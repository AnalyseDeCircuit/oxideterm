use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::Result;

use crate::{
    ConnectionStore, SSH_CONFIG_TAG, SSH_PROXY_COMMAND_TAG, SavedAuth, SavedConnection,
    SshConfigHost, list_ssh_config_hosts_from_path, saved_connection_from_ssh_host,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SshConfigSyncOutcome {
    pub imported: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

pub struct SshConfigSyncService {
    stop_tx: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SshConfigSyncService {
    pub fn start(
        connection_store_path: PathBuf,
        ssh_config_path: PathBuf,
        interval: Duration,
    ) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel();
        let interval = interval.max(Duration::from_secs(1));
        let worker = thread::Builder::new()
            .name("oxideterm-ssh-config-sync".to_string())
            .spawn(move || {
                let mut previous_hosts = None;
                loop {
                    let hosts = list_ssh_config_hosts_from_path(&ssh_config_path, &HashSet::new());
                    if let Ok(hosts) = hosts
                        && previous_hosts.as_ref() != Some(&hosts)
                    {
                        // The connection layer owns parsing, drift calculation,
                        // and persistence; GPUI only observes the resulting store file.
                        if sync_resolved_ssh_config_hosts(&connection_store_path, hosts.clone())
                            .is_ok()
                        {
                            previous_hosts = Some(hosts);
                        }
                    }
                    match stop_rx.recv_timeout(interval) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            })
            .ok();
        Self {
            stop_tx: Some(stop_tx),
            worker,
        }
    }
}

impl Drop for SshConfigSyncService {
    fn drop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn sync_ssh_config_path_into_store(
    connection_store_path: &Path,
    ssh_config_path: &Path,
) -> Result<SshConfigSyncOutcome> {
    let hosts = list_ssh_config_hosts_from_path(ssh_config_path, &HashSet::new())?;
    sync_resolved_ssh_config_hosts(connection_store_path, hosts)
}

fn sync_resolved_ssh_config_hosts(
    connection_store_path: &Path,
    hosts: Vec<SshConfigHost>,
) -> Result<SshConfigSyncOutcome> {
    let mut store = ConnectionStore::load(connection_store_path.to_path_buf())?;
    let mut pending = Vec::new();
    let mut outcome = SshConfigSyncOutcome::default();

    for host in hosts {
        let alias = host.alias.clone();
        let existing = store
            .connections()
            .iter()
            .find(|connection| connection.name.eq_ignore_ascii_case(&alias))
            .cloned();
        let Some(existing) = existing else {
            pending.push(saved_connection_from_ssh_host(host)?);
            outcome.imported.push(alias);
            continue;
        };
        if !existing.tags.iter().any(|tag| tag == SSH_CONFIG_TAG) {
            // A same-name manual connection always wins over automatic import.
            outcome.skipped.push(alias);
            continue;
        }

        let mut resolved = saved_connection_from_ssh_host(host)?;
        if ssh_config_fields_match(&existing, &resolved) {
            outcome.skipped.push(alias);
            continue;
        }
        resolved.id = existing.id;
        resolved.group = existing.group;
        resolved.color = existing.color;
        resolved.icon = existing.icon;
        resolved.tags = merged_ssh_config_tags(&existing.tags, &resolved.tags);
        resolved.options = existing.options;
        resolved.upstream_proxy = existing.upstream_proxy;
        resolved.post_connect_command = existing.post_connect_command;
        pending.push(resolved);
        outcome.updated.push(alias);
    }

    if !pending.is_empty() {
        store.upsert_imported_connections_transaction(pending)?;
    }
    Ok(outcome)
}

fn ssh_config_fields_match(existing: &SavedConnection, resolved: &SavedConnection) -> bool {
    existing.host == resolved.host
        && existing.port == resolved.port
        && existing.username == resolved.username
        && auth_source_matches(&existing.auth, &resolved.auth)
        && existing.proxy_chain.len() == resolved.proxy_chain.len()
        && existing
            .proxy_chain
            .iter()
            .zip(&resolved.proxy_chain)
            .all(|(existing, resolved)| {
                existing.host == resolved.host
                    && existing.port == resolved.port
                    && existing.username == resolved.username
                    && auth_source_matches(&existing.auth, &resolved.auth)
            })
        && has_proxy_command_tag(&existing.tags) == has_proxy_command_tag(&resolved.tags)
}

fn has_proxy_command_tag(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag == SSH_PROXY_COMMAND_TAG)
}

fn merged_ssh_config_tags(existing: &[String], resolved: &[String]) -> Vec<String> {
    let mut tags = existing
        .iter()
        .filter(|tag| tag.as_str() != SSH_PROXY_COMMAND_TAG)
        .cloned()
        .collect::<Vec<_>>();
    if has_proxy_command_tag(resolved) {
        tags.push(SSH_PROXY_COMMAND_TAG.to_string());
    }
    tags
}

fn auth_source_matches(existing: &SavedAuth, resolved: &SavedAuth) -> bool {
    match (existing, resolved) {
        (SavedAuth::Agent, SavedAuth::Agent)
        | (SavedAuth::KeyboardInteractive, SavedAuth::KeyboardInteractive) => true,
        (
            SavedAuth::Key {
                key_path: existing_path,
                ..
            },
            SavedAuth::Key {
                key_path: resolved_path,
                ..
            },
        ) => existing_path == resolved_path,
        (
            SavedAuth::Certificate {
                key_path: existing_key,
                cert_path: existing_cert,
                ..
            },
            SavedAuth::Certificate {
                key_path: resolved_key,
                cert_path: resolved_cert,
                ..
            },
        ) => existing_key == resolved_key && existing_cert == resolved_cert,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SaveConnectionRequest, SavedUpstreamProxyPolicy};
    use uuid::Uuid;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("oxideterm-ssh-sync-{name}-{}", Uuid::new_v4()))
    }

    fn write_config(path: &Path, hostname: &str) {
        std::fs::write(
            path,
            format!("Host production\n  HostName {hostname}\n  User deploy\n  Port 2222\n"),
        )
        .unwrap();
    }

    #[test]
    fn sync_imports_new_hosts_and_updates_only_managed_connections() {
        let directory = temp_path("managed-update");
        std::fs::create_dir_all(&directory).unwrap();
        let config_path = directory.join("config");
        let store_path = directory.join("connections.json");
        write_config(&config_path, "old.example.com");

        let first = sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        assert_eq!(first.imported, vec!["production"]);
        let mut store = ConnectionStore::load(store_path.clone()).unwrap();
        let original = store.connections()[0].clone();
        store
            .move_to_group(&[original.id.clone()], Some("Custom Group"))
            .unwrap();

        write_config(&config_path, "new.example.com");
        let second = sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        assert_eq!(second.updated, vec!["production"]);
        let store = ConnectionStore::load(store_path).unwrap();
        let updated = &store.connections()[0];
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.host, "new.example.com");
        assert_eq!(updated.group.as_deref(), Some("Custom Group"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn sync_never_overwrites_a_same_name_manual_connection() {
        let directory = temp_path("manual-conflict");
        std::fs::create_dir_all(&directory).unwrap();
        let config_path = directory.join("config");
        let store_path = directory.join("connections.json");
        write_config(&config_path, "config.example.com");
        let mut store = ConnectionStore::load(store_path.clone()).unwrap();
        store
            .upsert(SaveConnectionRequest {
                id: None,
                name: "production".to_string(),
                group: None,
                host: "manual.example.com".to_string(),
                port: 22,
                username: "admin".to_string(),
                auth: SavedAuth::Agent,
                proxy_chain: Vec::new(),
                upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
                color: None,
                icon: None,
                tags: Vec::new(),
                agent_forwarding: false,
                legacy_ssh_compatibility: false,
                post_connect_command: None,
            })
            .unwrap();

        let outcome = sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();

        assert_eq!(outcome.skipped, vec!["production"]);
        let store = ConnectionStore::load(store_path).unwrap();
        assert_eq!(store.connections()[0].host, "manual.example.com");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn sync_updates_only_the_proxy_command_capability_marker() {
        let existing = vec![SSH_CONFIG_TAG.to_string(), "custom".to_string()];
        let resolved = vec![
            SSH_CONFIG_TAG.to_string(),
            SSH_PROXY_COMMAND_TAG.to_string(),
        ];

        let added = merged_ssh_config_tags(&existing, &resolved);
        let removed = merged_ssh_config_tags(&added, &[SSH_CONFIG_TAG.to_string()]);

        assert_eq!(
            added,
            [
                SSH_CONFIG_TAG.to_string(),
                "custom".to_string(),
                SSH_PROXY_COMMAND_TAG.to_string(),
            ]
        );
        assert_eq!(removed, [SSH_CONFIG_TAG.to_string(), "custom".to_string()]);
    }
}
