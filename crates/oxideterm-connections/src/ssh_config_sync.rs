use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::Result;

use crate::{
    ConnectionStore, SSH_CONFIG_TAG, SSH_PROXY_COMMAND_TAG, SSH_REMOTE_COMMAND_TAG, SavedAuth,
    SavedConnection, SshConfigHost, list_ssh_config_hosts_from_path,
    saved_connection_from_ssh_host,
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
        let existing_remote_command_managed = has_remote_command_tag(&existing.tags);
        let resolved_remote_command_managed = has_remote_command_tag(&resolved.tags);
        let resolved_post_connect_command = resolved.options.post_connect_command.take();
        let resolved_agent_forwarding = resolved.options.agent_forwarding;
        let resolved_connect_timeout_seconds = resolved.options.connect_timeout_seconds;
        let resolved_identity_agent = resolved.options.identity_agent.clone();
        let resolved_agent_forwarding_socket = resolved.options.agent_forwarding_socket.clone();
        let resolved_x11_forwarding = resolved.options.x11_forwarding;
        resolved.options = existing.options;
        // These fields remain owned by the imported OpenSSH config while
        // unrelated application-specific connection options stay untouched.
        resolved.options.connect_timeout_seconds = resolved_connect_timeout_seconds;
        resolved.options.agent_forwarding = resolved_agent_forwarding;
        resolved.options.identity_agent = resolved_identity_agent;
        resolved.options.agent_forwarding_socket = resolved_agent_forwarding_socket;
        resolved.options.x11_forwarding = resolved_x11_forwarding;
        if existing_remote_command_managed || resolved_remote_command_managed {
            // The marker transfers ownership back to OpenSSH config, including
            // clearing a command when RemoteCommand is removed or set to none.
            resolved.options.post_connect_command = resolved_post_connect_command;
            resolved.post_connect_command = None;
        } else {
            resolved.post_connect_command = existing.post_connect_command;
        }
        resolved.upstream_proxy = existing.upstream_proxy;
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
        && existing.options.connect_timeout_seconds == resolved.options.connect_timeout_seconds
        && existing.options.agent_forwarding == resolved.options.agent_forwarding
        && existing.options.identity_agent == resolved.options.identity_agent
        && existing.options.agent_forwarding_socket == resolved.options.agent_forwarding_socket
        && existing.options.x11_forwarding == resolved.options.x11_forwarding
        && remote_command_fields_match(existing, resolved)
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
                    && existing.agent_forwarding == resolved.agent_forwarding
                    && existing.identity_agent == resolved.identity_agent
                    && existing.agent_forwarding_socket == resolved.agent_forwarding_socket
            })
        && has_proxy_command_tag(&existing.tags) == has_proxy_command_tag(&resolved.tags)
}

fn has_proxy_command_tag(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag == SSH_PROXY_COMMAND_TAG)
}

fn has_remote_command_tag(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag == SSH_REMOTE_COMMAND_TAG)
}

fn remote_command_fields_match(existing: &SavedConnection, resolved: &SavedConnection) -> bool {
    let existing_managed = has_remote_command_tag(&existing.tags);
    let resolved_managed = has_remote_command_tag(&resolved.tags);
    existing_managed == resolved_managed
        && (!resolved_managed || existing.post_connect_command() == resolved.post_connect_command())
}

fn merged_ssh_config_tags(existing: &[String], resolved: &[String]) -> Vec<String> {
    let mut tags = existing
        .iter()
        .filter(|tag| !matches!(tag.as_str(), SSH_PROXY_COMMAND_TAG | SSH_REMOTE_COMMAND_TAG))
        .cloned()
        .collect::<Vec<_>>();
    if has_proxy_command_tag(resolved) {
        tags.push(SSH_PROXY_COMMAND_TAG.to_string());
    }
    if has_remote_command_tag(resolved) {
        tags.push(SSH_REMOTE_COMMAND_TAG.to_string());
    }
    tags
}

fn auth_source_matches(existing: &SavedAuth, resolved: &SavedAuth) -> bool {
    if existing.gssapi_options() != resolved.gssapi_options() {
        return false;
    }
    let existing = existing.conventional_fallback();
    let resolved = resolved.conventional_fallback();
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
    use crate::{ConnectionTerminalOptions, SaveConnectionRequest, SavedUpstreamProxyPolicy};
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
    fn sync_updates_agent_options_owned_by_open_ssh_config() {
        let directory = temp_path("agent-options");
        std::fs::create_dir_all(&directory).unwrap();
        let config_path = directory.join("config");
        let store_path = directory.join("connections.json");
        std::fs::write(
            &config_path,
            "Host production\n  HostName prod.example.com\n  ForwardAgent no\n  IdentityAgent ~/.ssh/agent-a.sock\n",
        )
        .unwrap();

        sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        std::fs::write(
            &config_path,
            "Host production\n  HostName prod.example.com\n  ForwardAgent yes\n  IdentityAgent ~/.ssh/agent-b.sock\n",
        )
        .unwrap();

        let outcome = sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        let store = ConnectionStore::load(&store_path).unwrap();
        let connection = &store.connections()[0];

        assert_eq!(outcome.updated, vec!["production"]);
        assert!(connection.options.agent_forwarding);
        assert!(
            connection
                .options
                .identity_agent
                .as_deref()
                .is_some_and(|path| path.ends_with("/.ssh/agent-b.sock"))
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn sync_tracks_remote_command_without_overwriting_app_owned_commands() {
        let directory = temp_path("remote-command");
        std::fs::create_dir_all(&directory).unwrap();
        let config_path = directory.join("config");
        let store_path = directory.join("connections.json");
        std::fs::write(
            &config_path,
            "Host production\n  HostName prod.example.com\n  RemoteCommand echo first\n",
        )
        .unwrap();

        sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        let store = ConnectionStore::load(&store_path).unwrap();
        assert_eq!(
            store.connections()[0].post_connect_command(),
            Some("echo first")
        );
        assert!(has_remote_command_tag(&store.connections()[0].tags));

        std::fs::write(
            &config_path,
            "Host production\n  HostName prod.example.com\n  RemoteCommand none\n",
        )
        .unwrap();
        sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        let store = ConnectionStore::load(&store_path).unwrap();
        assert!(store.connections()[0].post_connect_command().is_none());
        assert!(has_remote_command_tag(&store.connections()[0].tags));

        std::fs::write(
            &config_path,
            "Host production\n  HostName prod.example.com\n",
        )
        .unwrap();
        sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        let mut store = ConnectionStore::load(&store_path).unwrap();
        assert!(!has_remote_command_tag(&store.connections()[0].tags));
        let mut connection = store.connections()[0].clone();
        connection.options.post_connect_command = Some("echo app-owned".to_string());
        store
            .upsert_imported_connections_transaction(vec![connection])
            .unwrap();

        let outcome = sync_ssh_config_path_into_store(&store_path, &config_path).unwrap();
        let store = ConnectionStore::load(&store_path).unwrap();
        assert_eq!(outcome.skipped, vec!["production"]);
        assert_eq!(
            store.connections()[0].post_connect_command(),
            Some("echo app-owned")
        );
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
                notes: None,
                host: "manual.example.com".to_string(),
                port: 22,
                username: "admin".to_string(),
                auth: SavedAuth::Agent,
                proxy_chain: Vec::new(),
                upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
                proxy_command: None,
                color: None,
                icon_background_color: None,
                icon: None,
                tags: Vec::new(),
                connect_timeout_seconds: crate::DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
                agent_forwarding: false,
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: false,
                ssh_algorithms: crate::SshAlgorithmPreferences::default(),
                x11_forwarding: crate::ConnectionX11ForwardingOptions::default(),
                dedicated_new_terminal_connection: false,
                post_connect_command: None,
                terminal: ConnectionTerminalOptions::default(),
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
