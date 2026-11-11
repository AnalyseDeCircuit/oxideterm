// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime},
};

use dashmap::DashMap;
use oxideterm_connections::SshAlgorithmPreferences;
use russh::{
    client,
    keys::{
        PublicKey, PublicKeyBase64, PublicKeyOrCertificate, parse_public_key_base64,
        ssh_key::HashAlg,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    ProxyCommandConfig, SshTransportError,
    local_paths::default_ssh_dir,
    transport::{BoxedSshForwardStream, dial_proxy_command},
    upstream_proxy::{UpstreamProxyConfig, dial_initial_tcp},
};

const CACHE_TTL_SECS: u64 = 3600;
const MAX_CACHE_ENTRIES: usize = 500;

mod management;
pub use management::{HostKeyReplacement, KnownHostEntry, KnownHostsSnapshot, list_known_hosts};

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
        let lookup_keys = vec![lookup_key.clone()];
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
    management::append_key(path, host, key_type, key_data)
}

fn rewrite_without_host_key(
    path: &Path,
    remove_hosts: &[String],
    remove_key_type: &str,
    remove_fingerprint: &str,
) -> Result<bool, SshTransportError> {
    management::rewrite_key(
        path,
        remove_hosts,
        remove_key_type,
        remove_fingerprint,
        None,
    )
}

pub fn verify_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
) -> Result<HostKeyVerification, SshTransportError> {
    if HOST_KEY_CACHE.get_verified(host, port).as_deref()
        == Some(public_key_fingerprint(server_public_key).as_str())
    {
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
    let mut audit = oxideterm_audit::AuditOperation::begin(
        oxideterm_audit::AuditCategory::Security,
        "host_trust_add",
        Some(host),
        None,
    );
    audit.summary(&format!("port={port},fingerprint={fingerprint}"));
    let audit_result = (|| {
        let mut store = KnownHostsStore::new()?;
        store.add_host(host, port, server_public_key)?;
        accept_host_key_for_session(host, port, fingerprint);
        Ok(())
    })();
    audit.result(&audit_result);
    audit_result
}

pub fn remove_host_key(
    host: &str,
    port: u16,
    key_type: &str,
    expected_fingerprint: &str,
) -> Result<(), SshTransportError> {
    let mut audit = oxideterm_audit::AuditOperation::begin(
        oxideterm_audit::AuditCategory::Security,
        "host_trust_remove",
        Some(host),
        None,
    );
    audit.summary(&format!(
        "port={port},key_type={key_type},fingerprint={expected_fingerprint}"
    ));
    let audit_result =
        (|| KnownHostsStore::new()?.remove_host_key(host, port, key_type, expected_fingerprint))();
    audit.result(&audit_result);
    audit_result
}

fn default_known_hosts_path() -> Result<PathBuf, SshTransportError> {
    Ok(known_hosts_path_from_ssh_dir(default_ssh_dir()))
}

fn known_hosts_path_from_ssh_dir(ssh_dir: Option<PathBuf>) -> PathBuf {
    ssh_dir
        .map(|ssh_dir| ssh_dir.join("known_hosts"))
        .unwrap_or_else(|| PathBuf::from("~/.ssh/known_hosts"))
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
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey {
            key: server_public_key,
            ..
        } = server_key
        else {
            // Host certificates require CA, principal, validity, and revocation
            // checks; treating the embedded key as a raw known-hosts key would
            // silently bypass that trust policy.
            return Ok(false);
        };
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

fn preflight_client_config(
    timeout_secs: u64,
    legacy_compatibility: bool,
    algorithms: &SshAlgorithmPreferences,
) -> Result<client::Config, crate::SshAlgorithmPreferenceError> {
    // Host-key capture happens after algorithm negotiation, so preflight must use the login policy.
    Ok(client::Config {
        inactivity_timeout: Some(Duration::from_secs(timeout_secs)),
        preferred: crate::preferred_algorithms(legacy_compatibility, algorithms)?,
        gex: crate::algorithms::preferred_gex(legacy_compatibility, algorithms),
        ..client::Config::default()
    })
}

pub async fn check_host_key(host: &str, port: u16, timeout_secs: u64) -> HostKeyStatus {
    check_host_key_with_upstream_proxy(
        host,
        port,
        timeout_secs,
        None,
        false,
        &SshAlgorithmPreferences::default(),
    )
    .await
}

pub async fn check_host_key_with_upstream_proxy(
    host: &str,
    port: u16,
    timeout_secs: u64,
    upstream_proxy: Option<&UpstreamProxyConfig>,
    legacy_compatibility: bool,
    algorithms: &SshAlgorithmPreferences,
) -> HostKeyStatus {
    check_host_key_with_route(
        host,
        port,
        timeout_secs,
        upstream_proxy,
        None,
        legacy_compatibility,
        algorithms,
    )
    .await
}

pub async fn check_host_key_with_route(
    host: &str,
    port: u16,
    timeout_secs: u64,
    upstream_proxy: Option<&UpstreamProxyConfig>,
    proxy_command: Option<&ProxyCommandConfig>,
    legacy_compatibility: bool,
    algorithms: &SshAlgorithmPreferences,
) -> HostKeyStatus {
    let stream: Result<BoxedSshForwardStream, SshTransportError> = match proxy_command {
        Some(_) if upstream_proxy.is_some() => Err(SshTransportError::ConnectionFailed(
            "ProxyCommand cannot be combined with an upstream proxy".to_string(),
        )),
        Some(command) => dial_proxy_command(command)
            .await
            .map(|stream| Box::new(stream) as BoxedSshForwardStream),
        None => dial_initial_tcp(host, port, timeout_secs, upstream_proxy)
            .await
            .map(|stream| Box::new(stream) as BoxedSshForwardStream),
    };
    let stream = match stream {
        Ok(stream) => stream,
        Err(error) => {
            return HostKeyStatus::Error {
                message: error.to_string(),
            };
        }
    };

    let handler = PreflightHandler::new(host.to_string(), port);
    let status = Arc::clone(&handler.status);
    let config = match preflight_client_config(timeout_secs, legacy_compatibility, algorithms) {
        Ok(config) => config,
        Err(error) => {
            return HostKeyStatus::Error {
                message: error.to_string(),
            };
        }
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
    legacy_compatibility: bool,
    algorithms: &SshAlgorithmPreferences,
) -> HostKeyStatus {
    let handler = PreflightHandler::new(host.to_string(), port);
    let status = Arc::clone(&handler.status);
    let config = match preflight_client_config(timeout_secs, legacy_compatibility, algorithms) {
        Ok(config) => config,
        Err(error) => {
            return HostKeyStatus::Error {
                message: error.to_string(),
            };
        }
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

    pub(super) fn sample_public_key() -> PublicKey {
        parse_public_key_base64(
            "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
        )
        .unwrap()
    }

    pub(super) fn alternate_public_key() -> PublicKey {
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
    fn known_hosts_store_verifies_alias_and_reports_changed_key() {
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
        assert_eq!(
            store.verify("example.com", 2222, &key),
            HostKeyVerification::Unknown {
                fingerprint: public_key_fingerprint(&key),
                key_type: public_key_type(&key),
            }
        );
        let alternate = alternate_public_key();
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
    fn accepted_host_key_cache_does_not_skip_the_live_server_probe() {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_HOST_KEY_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Portable runtime paths are process-wide; never write the user's trust file.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            fs::hard_link(&executable, &child).unwrap();
            fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .args([
                    "host_key::tests::accepted_host_key_cache_does_not_skip_the_live_server_probe",
                    "--exact",
                    "--nocapture",
                ])
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let path = default_known_hosts_path().unwrap();
        assert!(path.starts_with(PathBuf::from(fixture_dir)));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            struct Peer;
            impl russh::server::Handler for Peer {
                type Error = russh::Error;
                async fn auth_none(&mut self, _: &str) -> Result<russh::server::Auth, Self::Error> {
                    Ok(russh::server::Auth::Accept)
                }
            }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let old_key = sample_public_key();
            let new_key = russh::keys::PrivateKey::random(
                &mut rand10::rng(),
                russh::keys::Algorithm::Ed25519,
            )
            .unwrap();
            let new_public_key = new_key.public_key().clone();
            let expected = public_key_fingerprint(&old_key);
            let actual = public_key_fingerprint(&new_public_key);
            learn_host_key("127.0.0.1", port, &old_key).unwrap();
            let original = fs::read_to_string(&path).unwrap();
            assert_eq!(
                verify_host_key("127.0.0.1", port, &old_key).unwrap(),
                HostKeyVerification::Verified
            );
            let server = tokio::spawn(async move {
                let config = Arc::new(russh::server::Config {
                    keys: vec![new_key],
                    ..Default::default()
                });
                let mut sessions = tokio::task::JoinSet::new();
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let config = config.clone();
                    sessions.spawn(async move {
                        if let Ok(session) = russh::server::run_stream(config, stream, Peer).await {
                            let _ = session.await;
                        }
                    });
                }
            });
            let changed = HostKeyStatus::Changed {
                expected_fingerprint: expected.clone(),
                actual_fingerprint: actual.clone(),
                key_type: "ssh-ed25519".into(),
            };
            assert_eq!(check_host_key("127.0.0.1", port, 5).await, changed);

            let mut config = crate::SshConfig::password("127.0.0.1", port, "fixture", "unused");
            config.timeout_secs = 5;
            config.strict_host_key_checking = true;
            config.expected_host_key_fingerprint = Some(actual.clone());
            config.trust_host_key = Some(true);
            config.host_key_replacement = Some(HostKeyReplacement {
                host: config.host.clone(),
                port,
                key_type: "ssh-ed25519".into(),
                previous_fingerprint: expected.clone(),
                fingerprint: expected.clone(),
            });
            let rejected = crate::SshTransportClient::new(config.clone())
                .test_connection()
                .await;
            assert!(matches!(
                rejected,
                Err(SshTransportError::HostKeyChanged { .. })
            ));
            assert_eq!(fs::read_to_string(&path).unwrap(), original);

            config.host_key_replacement.as_mut().unwrap().fingerprint = actual.clone();
            assert!(
                !serde_json::to_value(&config)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .contains_key("host_key_replacement")
            );
            crate::SshTransportClient::new(config)
                .test_connection()
                .await
                .unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                format!(
                    "[127.0.0.1]:{port} ssh-ed25519 {}\n",
                    new_public_key.public_key_base64()
                )
            );
            assert_eq!(
                check_host_key("127.0.0.1", port, 5).await,
                HostKeyStatus::Verified
            );
            remove_host_key("127.0.0.1", port, "ssh-ed25519", &actual).unwrap();
            assert_eq!(
                check_host_key("127.0.0.1", port, 5).await,
                HostKeyStatus::Unknown {
                    fingerprint: actual,
                    key_type: "ssh-ed25519".into(),
                }
            );
            server.abort();
            let _ = server.await;
        });
    }

    #[test]
    fn preflight_uses_proxy_command_route_before_direct_network() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        HOST_KEY_CACHE.clear();
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let status = runtime.block_on(check_host_key_with_route(
            "proxy-command-only.invalid",
            22,
            1,
            None,
            Some(&ProxyCommandConfig::AuthorizationRequired),
            false,
            &SshAlgorithmPreferences::default(),
        ));

        let HostKeyStatus::Error { message } = status else {
            panic!("unauthorized ProxyCommand preflight should fail explicitly");
        };
        assert!(message.contains("requires explicit authorization"));
        HOST_KEY_CACHE.clear();
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
