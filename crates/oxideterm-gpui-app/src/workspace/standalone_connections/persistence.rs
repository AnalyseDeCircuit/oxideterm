use super::*;
use oxideterm_connections::{
    MoshProfile, RemoteDesktopProfile, SavedAuth, SavedProxyHop, SavedUpstreamProxyAuth,
    SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct SessionSnapshot {
    id: String,
    title: String,
    kind: StandaloneConnectionKind,
    launch: LaunchSnapshot,
}

// Only this projection crosses the disk boundary. Runtime launch values own credentials,
// provider processes and attempt identities and must never derive Serialize.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LaunchSnapshot {
    Saved {
        profile_id: String,
    },
    Serial {
        config: SerialSessionConfig,
        terminal: ConnectionTerminalOptions,
    },
    Telnet {
        config: TelnetSessionConfig,
        terminal: ConnectionTerminalOptions,
    },
    Mosh {
        profile: MoshProfile,
    },
    RemoteDesktop {
        profile: RemoteDesktopProfile,
    },
}

impl StandaloneConnectionRegistry {
    pub(in crate::workspace) fn restore(path: PathBuf, store: &ConnectionStore) -> Self {
        let snapshots = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Vec<SessionSnapshot>>(&bytes) {
                Ok(snapshots) => snapshots,
                Err(_) => {
                    eprintln!("failed to parse standalone session snapshot");
                    Vec::new()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                eprintln!("failed to read standalone session snapshot: {error}");
                Vec::new()
            }
        };
        let mut ids = HashSet::new();
        let records = snapshots
            .into_iter()
            .filter_map(|snapshot| {
                if !ids.insert(snapshot.id.clone()) {
                    return None;
                }
                let launch = snapshot.launch.restore(snapshot.kind, store)?;
                Some(StandaloneConnectionRecord {
                    id: snapshot.id,
                    attempt_id: uuid::Uuid::new_v4().to_string(),
                    kind: snapshot.kind,
                    title: snapshot.title,
                    launch,
                    surface: None,
                    readiness: ActiveSessionReadiness::Disconnected,
                })
            })
            .collect();
        Self {
            records,
            snapshot_path: Some(path),
        }
    }

    pub(super) fn persist(&self) {
        let Some(path) = &self.snapshot_path else {
            return;
        };
        let snapshots = self
            .records
            .iter()
            .map(|record| SessionSnapshot {
                id: record.id.clone(),
                title: record.title.clone(),
                kind: record.kind,
                launch: LaunchSnapshot::from_record(record),
            })
            .collect::<Vec<_>>();
        let result = serde_json::to_vec_pretty(&snapshots)
            .map_err(std::io::Error::other)
            .and_then(|bytes| oxideterm_atomic_file::durable_write(path, &bytes));
        if let Err(error) = result {
            eprintln!("failed to persist standalone session snapshot: {error}");
        }
    }

    pub(in crate::workspace) fn connection_id_for_attempt(&self, attempt: &str) -> Option<String> {
        self.records
            .iter()
            .find(|record| record.attempt_id == attempt)
            .map(|record| record.id.clone())
    }

    pub(in crate::workspace) fn replace_launch_for_attempt(
        &mut self,
        attempt: &str,
        title: String,
        launch: StandaloneConnectionLaunch,
    ) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.attempt_id == attempt)
        {
            record.title = title;
            record.launch = launch;
            self.persist();
        }
    }
}

impl LaunchSnapshot {
    fn from_record(record: &StandaloneConnectionRecord) -> Self {
        match &record.launch {
            StandaloneConnectionLaunch::SavedSerial { profile_id, .. }
            | StandaloneConnectionLaunch::SavedTelnet { profile_id, .. }
            | StandaloneConnectionLaunch::SavedMosh { profile_id }
            | StandaloneConnectionLaunch::SavedRemoteDesktop { profile_id } => Self::Saved {
                profile_id: profile_id.clone(),
            },
            StandaloneConnectionLaunch::Serial {
                config,
                terminal_options,
            } => Self::Serial {
                config: config.clone(),
                terminal: terminal_options.clone(),
            },
            StandaloneConnectionLaunch::Telnet {
                config,
                terminal_options,
            } => Self::Telnet {
                config: config.clone(),
                terminal: terminal_options.clone(),
            },
            StandaloneConnectionLaunch::MoshPreflight { config, options } => {
                let mut profile = MoshProfile::new(
                    &record.title,
                    &config.host,
                    config.port,
                    &config.username,
                    auth_metadata(&config.auth),
                );
                profile.id = record.id.clone();
                profile.proxy_chain = config
                    .proxy_chain
                    .iter()
                    .flatten()
                    .map(|hop| SavedProxyHop {
                        host: hop.host.clone(),
                        port: hop.port,
                        username: hop.username.clone(),
                        auth: auth_metadata(&hop.auth),
                        agent_forwarding: hop.agent_forwarding,
                        identity_agent: hop.identity_agent.clone(),
                        agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                        ssh_algorithms: hop.ssh_algorithms.clone(),
                    })
                    .collect();
                profile.server_executable = options.server_executable.clone();
                profile.udp_host_override = options.udp_host_override.clone();
                profile.udp_port = options.udp_port;
                profile.ip_family = options.ip_family;
                profile.prediction = options.prediction;
                profile.locale = options.locale.clone();
                profile.terminal = options.terminal.clone();
                profile.identity_agent = config.identity_agent.clone();
                profile.legacy_ssh_compatibility = config.legacy_ssh_compatibility;
                profile.ssh_algorithms = config.ssh_algorithms.clone();
                Self::Mosh { profile }
            }
            StandaloneConnectionLaunch::RemoteDesktop {
                profile,
                ssh_gateway_connection_id,
                ..
            } => {
                let mut saved = RemoteDesktopProfile::new(
                    &record.title,
                    profile.protocol,
                    &profile.endpoint.host,
                    profile.endpoint.port,
                );
                saved.id = record.id.clone();
                saved.username = profile.username.clone();
                saved.domain = profile.domain.clone();
                saved.read_only = profile.read_only;
                saved.session_options = profile.session_options;
                saved.ssh_gateway_connection_id = ssh_gateway_connection_id.clone();
                if let Some(proxy) = &profile.socks_proxy {
                    saved.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
                        proxy: SavedUpstreamProxyConfig {
                            host: proxy.host.clone(),
                            port: proxy.port,
                            protocol: SavedUpstreamProxyProtocol::Socks5,
                            remote_dns: proxy.remote_dns,
                            no_proxy: proxy.no_proxy.clone(),
                            auth: proxy.auth.as_ref().map_or(
                                SavedUpstreamProxyAuth::None,
                                |auth| SavedUpstreamProxyAuth::Password {
                                    username: auth.username.clone(),
                                    keychain_id: None,
                                    plaintext_password: None,
                                },
                            ),
                        },
                    };
                }
                Self::RemoteDesktop { profile: saved }
            }
            StandaloneConnectionLaunch::RestoredMosh { profile } => Self::Mosh {
                profile: profile.clone(),
            },
            StandaloneConnectionLaunch::RestoredRemoteDesktop { profile } => Self::RemoteDesktop {
                profile: profile.clone(),
            },
        }
    }

    fn restore(
        self,
        kind: StandaloneConnectionKind,
        store: &ConnectionStore,
    ) -> Option<StandaloneConnectionLaunch> {
        Some(match self {
            Self::Saved { profile_id } => match kind {
                StandaloneConnectionKind::Serial => {
                    let profile = store
                        .serial_profiles()
                        .iter()
                        .find(|profile| profile.id == profile_id)?;
                    StandaloneConnectionLaunch::SavedSerial {
                        profile_id,
                        config: SerialSessionConfig {
                            port_path: profile.port_path.clone(),
                            baud_rate: profile.baud_rate,
                            data_bits: profile.data_bits,
                            stop_bits: profile.stop_bits,
                            parity: new_connection::terminal_serial_parity_from_profile(
                                &profile.parity,
                            ),
                            flow_control: new_connection::terminal_serial_flow_from_profile(
                                &profile.flow_control,
                            ),
                            runtime_options:
                                new_connection::terminal_serial_runtime_options_from_profile(
                                    profile,
                                ),
                        },
                        terminal_options: profile.terminal.clone(),
                    }
                }
                StandaloneConnectionKind::Telnet => {
                    let profile = store
                        .telnet_profiles()
                        .iter()
                        .find(|profile| profile.id == profile_id)?;
                    StandaloneConnectionLaunch::SavedTelnet {
                        profile_id,
                        config: TelnetSessionConfig {
                            host: profile.host.clone(),
                            port: profile.port,
                        },
                        terminal_options: profile.terminal.clone(),
                    }
                }
                StandaloneConnectionKind::Mosh => {
                    store.get_mosh_profile(&profile_id)?;
                    StandaloneConnectionLaunch::SavedMosh { profile_id }
                }
                StandaloneConnectionKind::Rdp | StandaloneConnectionKind::Vnc => {
                    store.get_remote_desktop_profile(&profile_id)?;
                    StandaloneConnectionLaunch::SavedRemoteDesktop { profile_id }
                }
            },
            Self::Serial { config, terminal } if kind == StandaloneConnectionKind::Serial => {
                StandaloneConnectionLaunch::Serial {
                    config,
                    terminal_options: terminal,
                }
            }
            Self::Telnet { config, terminal } if kind == StandaloneConnectionKind::Telnet => {
                StandaloneConnectionLaunch::Telnet {
                    config,
                    terminal_options: terminal,
                }
            }
            Self::Mosh { profile } if kind == StandaloneConnectionKind::Mosh => {
                StandaloneConnectionLaunch::RestoredMosh { profile }
            }
            Self::RemoteDesktop { profile }
                if matches!(
                    kind,
                    StandaloneConnectionKind::Rdp | StandaloneConnectionKind::Vnc
                ) =>
            {
                StandaloneConnectionLaunch::RestoredRemoteDesktop { profile }
            }
            _ => return None,
        })
    }
}

fn auth_metadata(auth: &AuthMethod) -> SavedAuth {
    match auth {
        AuthMethod::Password { .. } => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        },
        AuthMethod::Key {
            key_path,
            passphrase,
        } => SavedAuth::Key {
            key_path: key_path.clone(),
            has_passphrase: passphrase.is_some(),
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthMethod::ManagedKey { key_id, .. } => SavedAuth::ManagedKey {
            key_id: key_id.clone(),
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => SavedAuth::Certificate {
            key_path: key_path.clone(),
            cert_path: cert_path.clone(),
            has_passphrase: passphrase.is_some(),
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        AuthMethod::Agent => SavedAuth::Agent,
        AuthMethod::KeyboardInteractive => SavedAuth::KeyboardInteractive,
        AuthMethod::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback,
        } => SavedAuth::KerberosPreferred {
            server_identity: server_identity.clone(),
            delegate_credentials: *delegate_credentials,
            fallback: Box::new(auth_metadata(fallback)),
        },
    }
}

impl StandaloneConnectionRecord {
    fn reauthentication_form(&self) -> Option<new_connection::NewConnectionForm> {
        let mut form = match &self.launch {
            StandaloneConnectionLaunch::RestoredMosh { profile } => {
                let mut form = new_connection::form_from_mosh_profile(profile, String::new());
                form.mosh_profile_id = None;
                form
            }
            StandaloneConnectionLaunch::RestoredRemoteDesktop { profile } => {
                let mut form =
                    new_connection::form_from_remote_desktop_profile(profile, String::new());
                form.remote_desktop_profile_id = None;
                form
            }
            _ => return None,
        };
        form.save_password = false;
        form.standalone_connection_id = Some(self.id.clone());
        Some(form)
    }
}

impl WorkspaceApp {
    pub(super) fn open_restored_standalone_form(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self
            .standalone_connections
            .record(id)
            .and_then(StandaloneConnectionRecord::reauthentication_form)
        else {
            return false;
        };
        self.open_new_connection_form(window, cx);
        self.update_connection_form_state(cx, |state| state.form = Some(form));
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests;
