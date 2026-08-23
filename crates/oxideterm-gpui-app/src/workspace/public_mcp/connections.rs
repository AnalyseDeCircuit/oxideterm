use chrono::SecondsFormat;
use oxideterm_connections::{
    ConnectionCredentialSlot, ConnectionTerminalBackspaceSequence,
    ConnectionTerminalDeleteSequence, ConnectionTerminalEncoding, ConnectionTerminalOptions,
    ConnectionTerminalSessionLogPolicy, ConnectionX11ForwardingMode,
    ConnectionX11ForwardingOptions, DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
    DEFAULT_X11_UNTRUSTED_TIMEOUT_SECONDS, MoshIpFamily, MoshPredictionMode, MoshUdpPortSelection,
    SaveConnectionRequest, SaveMoshProfileRequest, SaveRemoteDesktopProfileRequest,
    SaveSerialProfileRequest, SaveTelnetProfileRequest, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy,
    SavedUpstreamProxyProtocol, SecretString, SerialFlowControl, SerialParity,
};
use oxideterm_public_mcp::{
    ClientRef, ConnectionRef, DomainRequest, PublicConnectionAuth, PublicCredentialSlot,
    PublicMoshIpFamily, PublicMoshPredictionMode, PublicMoshUdpPortSelection,
    PublicRemoteDesktopOptions, PublicSavedConnectionProfile, PublicSerialFlowControl,
    PublicSerialParity, PublicTerminalBackspaceSequence, PublicTerminalDeleteSequence,
    PublicTerminalEncoding, PublicTerminalOptions, PublicTerminalSessionLogPolicy, PublicToolCall,
    PublicUpstreamProxy, PublicUpstreamProxyProtocol, PublicVncCompression, PublicVncImageQuality,
    PublicVncSecurityPolicy, PublicVncSessionMode, PublicX11ForwardingMode, ToolEnvelope,
};
use oxideterm_remote_desktop::{
    RemoteDesktopAudioOptions, RemoteDesktopClipboardOptions, RemoteDesktopDisplayOptions,
    RemoteDesktopProtocol, RemoteDesktopRdpOptions, RemoteDesktopSessionOptions,
    RemoteDesktopVncCompression, RemoteDesktopVncImageQuality, RemoteDesktopVncOptions,
    RemoteDesktopVncSecurityPolicy, RemoteDesktopVncSessionMode,
};
use serde_json::{Value, json};

use super::{
    CONNECTION_KEY_DESKTOP_PREFIX, CONNECTION_KEY_MOSH_PREFIX, CONNECTION_KEY_SERIAL_PREFIX,
    CONNECTION_KEY_SSH_PREFIX, CONNECTION_KEY_TELNET_PREFIX, PublicMcpWorkspaceBridge,
    WorkspaceApp, finish_serialized,
};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_save_connection(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::SaveConnection(args) = &request.call else {
            return;
        };
        let existing_key = match &args.connection_ref {
            Some(connection_ref) => match self.public_mcp.connection_key(
                &request.client_ref,
                connection_ref,
                &self.connection_store,
            ) {
                Some(key) => Some(key),
                None => {
                    request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
                    return;
                }
            },
            None => None,
        };
        if let Some(expected_revision) = args.expected_revision.as_deref() {
            let actual_revision = existing_key
                .as_deref()
                .and_then(|key| connection_revision(&self.connection_store, key));
            if actual_revision.as_deref() != Some(expected_revision) {
                request.finish(ToolEnvelope::failed(
                    "The saved connection changed after the supplied revision",
                ));
                return;
            }
        } else if existing_key.is_some() {
            request.finish(ToolEnvelope::failed(
                "Updating a saved connection requires expected_revision",
            ));
            return;
        }

        let created = existing_key.is_none();
        let saved_key = match save_profile(
            &mut self.connection_store,
            existing_key.as_deref(),
            &args.profile,
        ) {
            Ok(saved_key) => saved_key,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let connection_ref = self
            .public_mcp
            .ensure_connection_ref(&request.client_ref, saved_key.clone());
        let revision = connection_revision(&self.connection_store, &saved_key)
            .unwrap_or_else(|| "unavailable".to_owned());
        // Public MCP mutations share the same Cloud Sync invalidation boundary as UI edits.
        self.queue_cloud_sync_dirty_refresh(cx);
        cx.notify();
        finish_serialized(
            request,
            json!({
                "connection_ref": connection_ref,
                "revision": revision,
                "created": created,
            }),
        );
    }

    pub(super) fn handle_public_mcp_remove_connection(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::RemoveConnection(args) = &request.call else {
            return;
        };
        let Some(connection_key) = self.public_mcp.connection_key(
            &request.client_ref,
            &args.connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            return;
        };
        if !args.forget_credentials
            && connection_has_configured_credentials(&self.connection_store, &connection_key)
        {
            request.finish(ToolEnvelope::failed(
                "The profile has protected credentials; set forget_credentials=true to remove both",
            ));
            return;
        }
        let removed = match remove_profile(&mut self.connection_store, &connection_key) {
            Ok(removed) => removed,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if removed {
            self.public_mcp.remove_connection_ref(
                &request.client_ref,
                &args.connection_ref,
                &connection_key,
            );
            // Removing saved metadata does not disconnect an already leased physical node.
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
        finish_serialized(request, json!({ "removed": removed }));
    }

    pub(super) fn handle_public_mcp_credential_status(&mut self, request: DomainRequest) {
        let PublicToolCall::CredentialStatus(args) = &request.call else {
            return;
        };
        let connection_ref = args.connection_ref.clone();
        let Some(connection_key) = self.public_mcp.connection_key(
            &request.client_ref,
            &connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            return;
        };
        let slots = credential_status(&self.connection_store, &connection_key);
        let revision = connection_revision(&self.connection_store, &connection_key);
        finish_serialized(
            request,
            json!({
                "connection_ref": connection_ref,
                "revision": revision,
                "slots": slots,
            }),
        );
    }

    pub(super) fn handle_public_mcp_store_credential(
        &mut self,
        mut request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::StoreCredential(args) = &mut request.call else {
            return;
        };
        let connection_ref = args.connection_ref.clone();
        let slot = args.slot;
        let secret = SecretString::from(std::mem::take(&mut args.new_secret));
        let Some(connection_key) = self.public_mcp.connection_key(
            &request.client_ref,
            &connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            return;
        };
        let stored =
            match store_credential(&mut self.connection_store, &connection_key, slot, &secret) {
                Ok(stored) => stored,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
        if stored {
            // Credential metadata is syncable even though the protected value never leaves storage.
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
        finish_serialized(
            request,
            json!({
                "connection_ref": connection_ref,
                "slot": slot,
                "stored": stored,
                "revision": connection_revision(&self.connection_store, &connection_key),
            }),
        );
    }

    pub(super) fn handle_public_mcp_forget_credential(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::ForgetCredential(args) = &request.call else {
            return;
        };
        let connection_ref = args.connection_ref.clone();
        let slot = args.slot;
        let Some(connection_key) = self.public_mcp.connection_key(
            &request.client_ref,
            &connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            return;
        };
        let forgotten = match forget_credential(&mut self.connection_store, &connection_key, slot) {
            Ok(forgotten) => forgotten,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if forgotten {
            // Forgetting a protected value also changes the syncable credential projection.
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
        finish_serialized(
            request,
            json!({
                "connection_ref": connection_ref,
                "slot": slot,
                "forgotten": forgotten,
                "revision": connection_revision(&self.connection_store, &connection_key),
            }),
        );
    }
}

impl PublicMcpWorkspaceBridge {
    fn remove_connection_ref(
        &mut self,
        client_ref: &ClientRef,
        connection_ref: &ConnectionRef,
        connection_key: &str,
    ) {
        if self
            .connection_ids
            .get(connection_ref)
            .is_some_and(|(owner, key)| owner == client_ref && key == connection_key)
        {
            self.connection_ids.remove(connection_ref);
            self.connection_refs
                .remove(&(client_ref.clone(), connection_key.to_owned()));
        }
    }
}

fn save_profile(
    store: &mut oxideterm_connections::ConnectionStore,
    existing_key: Option<&str>,
    profile: &PublicSavedConnectionProfile,
) -> Result<String, String> {
    match profile {
        PublicSavedConnectionProfile::Ssh(profile) => {
            let existing_id = typed_existing_id(existing_key, CONNECTION_KEY_SSH_PREFIX, "ssh")?;
            let existing = existing_id.and_then(|id| store.get(id));
            let proxy_chain = profile
                .proxy_chain
                .iter()
                .enumerate()
                .map(|(index, hop)| {
                    let existing_hop =
                        existing.and_then(|connection| connection.proxy_chain.get(index));
                    let matching_hop = existing_hop.filter(|existing_hop| {
                        existing_hop.host == hop.host.trim()
                            && existing_hop.port == hop.port
                            && existing_hop.username == hop.username.trim()
                    });
                    Ok(SavedProxyHop {
                        host: hop.host.clone(),
                        port: hop.port,
                        username: hop.username.clone(),
                        auth: saved_auth(&hop.auth, matching_hop.map(|hop| &hop.auth))?,
                        agent_forwarding: hop.agent_forwarding,
                        identity_agent: hop.identity_agent.clone(),
                        agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                        ssh_algorithms: matching_hop
                            .map(|hop| hop.ssh_algorithms.clone())
                            .unwrap_or_default(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let request = SaveConnectionRequest {
                id: existing_id.map(ToOwned::to_owned),
                name: profile.name.clone(),
                group: profile.group.clone(),
                notes: profile.notes.clone(),
                host: profile.host.clone(),
                port: profile.port,
                username: profile.username.clone(),
                auth: saved_auth(&profile.auth, existing.map(|connection| &connection.auth))?,
                proxy_chain,
                upstream_proxy: saved_upstream_proxy(
                    &profile.upstream_proxy,
                    existing.map(|connection| &connection.upstream_proxy),
                ),
                // Public MCP edits cannot read ProxyCommand text, so retain its local reference.
                proxy_command: existing.and_then(|connection| connection.proxy_command.clone()),
                color: profile.color.clone(),
                icon_background_color: profile.icon_background_color.clone(),
                icon: profile.icon.clone(),
                tags: profile.tags.clone(),
                connect_timeout_seconds: profile
                    .connect_timeout_seconds
                    .unwrap_or(DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS),
                agent_forwarding: profile.agent_forwarding,
                identity_agent: profile.identity_agent.clone(),
                agent_forwarding_socket: profile.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: profile.legacy_ssh_compatibility,
                ssh_algorithms: existing
                    .map(|connection| connection.options.ssh_algorithms.clone())
                    .unwrap_or_default(),
                dedicated_new_terminal_connection: profile.dedicated_new_terminal_connection,
                x11_forwarding: ConnectionX11ForwardingOptions {
                    enabled: profile.x11_forwarding.enabled,
                    mode: match profile.x11_forwarding.mode {
                        PublicX11ForwardingMode::Untrusted => {
                            ConnectionX11ForwardingMode::Untrusted
                        }
                        PublicX11ForwardingMode::Trusted => ConnectionX11ForwardingMode::Trusted,
                    },
                    untrusted_timeout_seconds: profile
                        .x11_forwarding
                        .untrusted_timeout_seconds
                        .unwrap_or(DEFAULT_X11_UNTRUSTED_TIMEOUT_SECONDS),
                },
                post_connect_command: profile.post_connect_command.clone(),
                terminal: terminal_options(&profile.terminal),
            };
            let saved = store.upsert(request).map_err(public_store_error)?;
            Ok(format!("{CONNECTION_KEY_SSH_PREFIX}{}", saved.id))
        }
        PublicSavedConnectionProfile::Serial(profile) => {
            let existing_id =
                typed_existing_id(existing_key, CONNECTION_KEY_SERIAL_PREFIX, "serial")?;
            let saved = store
                .upsert_serial_profile(SaveSerialProfileRequest {
                    id: existing_id.map(ToOwned::to_owned),
                    name: profile.name.clone(),
                    group: profile.group.clone(),
                    notes: profile.notes.clone(),
                    icon: profile.icon.clone(),
                    color: profile.color.clone(),
                    icon_background_color: profile.icon_background_color.clone(),
                    port_path: profile.port_path.clone(),
                    baud_rate: profile.baud_rate,
                    data_bits: profile.data_bits,
                    stop_bits: profile.stop_bits,
                    parity: profile.parity.map(|parity| match parity {
                        PublicSerialParity::None => SerialParity::None,
                        PublicSerialParity::Odd => SerialParity::Odd,
                        PublicSerialParity::Even => SerialParity::Even,
                    }),
                    flow_control: profile.flow_control.map(|flow_control| match flow_control {
                        PublicSerialFlowControl::None => SerialFlowControl::None,
                        PublicSerialFlowControl::Software => SerialFlowControl::Software,
                        PublicSerialFlowControl::Hardware => SerialFlowControl::Hardware,
                    }),
                    terminal: terminal_options(&profile.terminal),
                    connect_on_open: Some(profile.connect_on_open),
                })
                .map_err(public_store_error)?;
            Ok(format!("{CONNECTION_KEY_SERIAL_PREFIX}{}", saved.id))
        }
        PublicSavedConnectionProfile::Telnet(profile) => {
            let existing_id =
                typed_existing_id(existing_key, CONNECTION_KEY_TELNET_PREFIX, "telnet")?;
            let saved = store
                .upsert_telnet_profile(SaveTelnetProfileRequest {
                    id: existing_id.map(ToOwned::to_owned),
                    name: profile.name.clone(),
                    group: profile.group.clone(),
                    notes: profile.notes.clone(),
                    icon: profile.icon.clone(),
                    color: profile.color.clone(),
                    icon_background_color: profile.icon_background_color.clone(),
                    host: profile.host.clone(),
                    port: profile.port,
                    terminal: terminal_options(&profile.terminal),
                    connect_on_open: Some(profile.connect_on_open),
                })
                .map_err(public_store_error)?;
            Ok(format!("{CONNECTION_KEY_TELNET_PREFIX}{}", saved.id))
        }
        PublicSavedConnectionProfile::Mosh(profile) => {
            let existing_id = typed_existing_id(existing_key, CONNECTION_KEY_MOSH_PREFIX, "mosh")?;
            let existing = existing_id.and_then(|id| store.get_mosh_profile(id));
            let proxy_chain = profile
                .proxy_chain
                .iter()
                .enumerate()
                .map(|(index, hop)| {
                    let matching_hop = existing
                        .and_then(|profile| profile.proxy_chain.get(index))
                        .filter(|existing_hop| {
                            existing_hop.host == hop.host.trim()
                                && existing_hop.port == hop.port
                                && existing_hop.username == hop.username.trim()
                        });
                    Ok(SavedProxyHop {
                        host: hop.host.clone(),
                        port: hop.port,
                        username: hop.username.clone(),
                        auth: saved_auth(&hop.auth, matching_hop.map(|hop| &hop.auth))?,
                        agent_forwarding: hop.agent_forwarding,
                        identity_agent: hop.identity_agent.clone(),
                        agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                        ssh_algorithms: matching_hop
                            .map(|hop| hop.ssh_algorithms.clone())
                            .unwrap_or_default(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let saved = store
                .upsert_mosh_profile(SaveMoshProfileRequest {
                    id: existing_id.map(ToOwned::to_owned),
                    name: profile.name.clone(),
                    group: profile.group.clone(),
                    notes: profile.notes.clone(),
                    icon: profile.icon.clone(),
                    color: profile.color.clone(),
                    icon_background_color: profile.icon_background_color.clone(),
                    host: profile.host.clone(),
                    ssh_port: profile.ssh_port,
                    username: profile.username.clone(),
                    auth: saved_auth(&profile.auth, existing.map(|profile| &profile.auth))?,
                    proxy_chain,
                    server_executable: profile.server_executable.clone(),
                    udp_host_override: profile.udp_host_override.clone(),
                    udp_port: match profile.udp_port {
                        PublicMoshUdpPortSelection::Automatic => MoshUdpPortSelection::Automatic,
                        PublicMoshUdpPortSelection::Fixed { port } => {
                            MoshUdpPortSelection::Fixed { port }
                        }
                        PublicMoshUdpPortSelection::Range { start, end } => {
                            MoshUdpPortSelection::Range { start, end }
                        }
                    },
                    ip_family: match profile.ip_family {
                        PublicMoshIpFamily::Auto => MoshIpFamily::Auto,
                        PublicMoshIpFamily::Ipv4 => MoshIpFamily::Ipv4,
                        PublicMoshIpFamily::Ipv6 => MoshIpFamily::Ipv6,
                    },
                    prediction: match profile.prediction {
                        PublicMoshPredictionMode::Adaptive => MoshPredictionMode::Adaptive,
                        PublicMoshPredictionMode::Always => MoshPredictionMode::Always,
                        PublicMoshPredictionMode::Never => MoshPredictionMode::Never,
                    },
                    locale: profile.locale.clone(),
                    identity_agent: profile.identity_agent.clone(),
                    legacy_ssh_compatibility: profile.legacy_ssh_compatibility,
                    ssh_algorithms: existing
                        .map(|profile| profile.ssh_algorithms.clone())
                        .unwrap_or_default(),
                    terminal: terminal_options(&profile.terminal),
                })
                .map_err(public_store_error)?;
            Ok(format!("{CONNECTION_KEY_MOSH_PREFIX}{}", saved.id))
        }
        PublicSavedConnectionProfile::Rdp(profile) => {
            save_remote_desktop_profile(store, existing_key, profile, RemoteDesktopProtocol::Rdp)
        }
        PublicSavedConnectionProfile::Vnc(profile) => {
            save_remote_desktop_profile(store, existing_key, profile, RemoteDesktopProtocol::Vnc)
        }
    }
}

fn save_remote_desktop_profile(
    store: &mut oxideterm_connections::ConnectionStore,
    existing_key: Option<&str>,
    profile: &oxideterm_public_mcp::PublicRemoteDesktopProfile,
    protocol: RemoteDesktopProtocol,
) -> Result<String, String> {
    let existing_id = typed_existing_id(
        existing_key,
        CONNECTION_KEY_DESKTOP_PREFIX,
        "remote desktop",
    )?;
    let existing = existing_id.and_then(|id| store.get_remote_desktop_profile(id));
    if let Some(existing) = existing
        && existing.protocol != protocol
    {
        return Err("A saved RDP profile cannot be changed into VNC, or vice versa".to_owned());
    }
    let saved = store
        .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
            id: existing_id.map(ToOwned::to_owned),
            name: profile.name.clone(),
            group: profile.group.clone(),
            notes: profile.notes.clone(),
            icon: profile.icon.clone(),
            color: profile.color.clone(),
            icon_background_color: profile.icon_background_color.clone(),
            protocol,
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            domain: profile.domain.clone(),
            credential_ref: existing.and_then(|profile| profile.credential_ref.clone()),
            credential: None,
            clear_credential: false,
            ssh_gateway_connection_id: existing
                .and_then(|profile| profile.ssh_gateway_connection_id.clone()),
            read_only: profile.read_only,
            session_options: remote_desktop_options(&profile.options),
        })
        .map_err(public_store_error)?;
    Ok(format!("{CONNECTION_KEY_DESKTOP_PREFIX}{}", saved.id))
}

fn typed_existing_id<'a>(
    existing_key: Option<&'a str>,
    expected_prefix: &str,
    expected_kind: &str,
) -> Result<Option<&'a str>, String> {
    match existing_key {
        Some(key) => key
            .strip_prefix(expected_prefix)
            .map(Some)
            .ok_or_else(|| format!("The existing connection is not a {expected_kind} profile")),
        None => Ok(None),
    }
}

fn saved_auth(
    input: &PublicConnectionAuth,
    existing: Option<&SavedAuth>,
) -> Result<SavedAuth, String> {
    match input {
        PublicConnectionAuth::Password => Ok(SavedAuth::Password {
            keychain_id: match existing {
                Some(SavedAuth::Password { keychain_id, .. }) => keychain_id.clone(),
                _ => None,
            },
            plaintext_password: None,
        }),
        PublicConnectionAuth::Key { key_path } => {
            let key_path = required_text(Some(key_path), "key_path")?;
            let reference = match existing {
                Some(SavedAuth::Key {
                    key_path: existing_path,
                    passphrase_keychain_id,
                    ..
                }) if existing_path == key_path => passphrase_keychain_id.clone(),
                _ => None,
            };
            Ok(SavedAuth::Key {
                key_path: key_path.to_owned(),
                has_passphrase: reference.is_some(),
                passphrase_keychain_id: reference,
                plaintext_passphrase: None,
            })
        }
        PublicConnectionAuth::ManagedKey { managed_key_id } => {
            let key_id = required_text(Some(managed_key_id), "managed_key_id")?;
            let reference = match existing {
                Some(SavedAuth::ManagedKey {
                    key_id: existing_id,
                    passphrase_keychain_id,
                    ..
                }) if existing_id == key_id => passphrase_keychain_id.clone(),
                _ => None,
            };
            Ok(SavedAuth::ManagedKey {
                key_id: key_id.to_owned(),
                passphrase_keychain_id: reference,
                plaintext_passphrase: None,
            })
        }
        PublicConnectionAuth::Certificate {
            key_path,
            certificate_path,
        } => {
            let key_path = required_text(Some(key_path), "key_path")?;
            let cert_path = required_text(Some(certificate_path), "certificate_path")?;
            let reference = match existing {
                Some(SavedAuth::Certificate {
                    key_path: existing_key,
                    cert_path: existing_cert,
                    passphrase_keychain_id,
                    ..
                }) if existing_key == key_path && existing_cert == cert_path => {
                    passphrase_keychain_id.clone()
                }
                _ => None,
            };
            Ok(SavedAuth::Certificate {
                key_path: key_path.to_owned(),
                cert_path: cert_path.to_owned(),
                has_passphrase: reference.is_some(),
                passphrase_keychain_id: reference,
                plaintext_passphrase: None,
            })
        }
        PublicConnectionAuth::KeyboardInteractive => Ok(SavedAuth::KeyboardInteractive),
        PublicConnectionAuth::Agent => Ok(SavedAuth::Agent),
        PublicConnectionAuth::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback,
        } => Ok(SavedAuth::with_kerberos_preferred(
            saved_auth(fallback, existing.map(SavedAuth::conventional_fallback))?,
            server_identity
                .as_deref()
                .map(str::trim)
                .filter(|identity| !identity.is_empty())
                .map(ToOwned::to_owned),
            *delegate_credentials,
        )),
    }
}

fn saved_upstream_proxy(
    input: &PublicUpstreamProxy,
    existing: Option<&SavedUpstreamProxyPolicy>,
) -> SavedUpstreamProxyPolicy {
    match input {
        PublicUpstreamProxy::UseGlobal => SavedUpstreamProxyPolicy::UseGlobal,
        PublicUpstreamProxy::Direct => SavedUpstreamProxyPolicy::Direct,
        PublicUpstreamProxy::Custom {
            protocol,
            host,
            port,
            username,
            remote_dns,
            no_proxy,
        } => {
            let protocol = match protocol {
                PublicUpstreamProxyProtocol::Socks5 => SavedUpstreamProxyProtocol::Socks5,
                PublicUpstreamProxyProtocol::HttpConnect => SavedUpstreamProxyProtocol::HttpConnect,
            };
            let existing_reference = match existing {
                Some(SavedUpstreamProxyPolicy::Custom { proxy })
                    if proxy.protocol == protocol
                        && proxy.host == host.trim()
                        && proxy.port == *port =>
                {
                    match (&proxy.auth, username.as_deref()) {
                        (
                            SavedUpstreamProxyAuth::Password {
                                username: existing_username,
                                keychain_id,
                                ..
                            },
                            Some(username),
                        ) if existing_username == username.trim() => keychain_id.clone(),
                        _ => None,
                    }
                }
                _ => None,
            };
            let auth = username
                .as_deref()
                .map_or(SavedUpstreamProxyAuth::None, |username| {
                    SavedUpstreamProxyAuth::Password {
                        username: username.to_owned(),
                        keychain_id: existing_reference,
                        plaintext_password: None,
                    }
                });
            SavedUpstreamProxyPolicy::Custom {
                proxy: SavedUpstreamProxyConfig {
                    protocol,
                    host: host.clone(),
                    port: *port,
                    auth,
                    remote_dns: *remote_dns,
                    no_proxy: no_proxy.clone(),
                },
            }
        }
    }
}

fn terminal_options(options: &PublicTerminalOptions) -> ConnectionTerminalOptions {
    ConnectionTerminalOptions {
        encoding: options.encoding.map(|encoding| match encoding {
            PublicTerminalEncoding::Utf8 => ConnectionTerminalEncoding::Utf8,
            PublicTerminalEncoding::Gbk => ConnectionTerminalEncoding::Gbk,
            PublicTerminalEncoding::Gb18030 => ConnectionTerminalEncoding::Gb18030,
            PublicTerminalEncoding::Big5 => ConnectionTerminalEncoding::Big5,
            PublicTerminalEncoding::ShiftJis => ConnectionTerminalEncoding::ShiftJis,
            PublicTerminalEncoding::EucJp => ConnectionTerminalEncoding::EucJp,
            PublicTerminalEncoding::EucKr => ConnectionTerminalEncoding::EucKr,
            PublicTerminalEncoding::Windows1252 => ConnectionTerminalEncoding::Windows1252,
        }),
        backspace_sequence: options.backspace_sequence.map(|sequence| match sequence {
            PublicTerminalBackspaceSequence::Delete => ConnectionTerminalBackspaceSequence::Delete,
            PublicTerminalBackspaceSequence::ControlH => {
                ConnectionTerminalBackspaceSequence::ControlH
            }
        }),
        delete_sequence: options.delete_sequence.map(|sequence| match sequence {
            PublicTerminalDeleteSequence::Csi3Tilde => ConnectionTerminalDeleteSequence::Csi3Tilde,
            PublicTerminalDeleteSequence::Delete => ConnectionTerminalDeleteSequence::Delete,
            PublicTerminalDeleteSequence::ControlH => ConnectionTerminalDeleteSequence::ControlH,
        }),
        semantic_scheme: None,
        highlight_rule_set: None,
        session_log_policy: match options.session_log_policy {
            PublicTerminalSessionLogPolicy::Inherit => ConnectionTerminalSessionLogPolicy::Inherit,
            PublicTerminalSessionLogPolicy::Automatic => {
                ConnectionTerminalSessionLogPolicy::Automatic
            }
            PublicTerminalSessionLogPolicy::Manual => ConnectionTerminalSessionLogPolicy::Manual,
            PublicTerminalSessionLogPolicy::Disabled => {
                ConnectionTerminalSessionLogPolicy::Disabled
            }
        },
    }
}

fn remote_desktop_options(options: &PublicRemoteDesktopOptions) -> RemoteDesktopSessionOptions {
    RemoteDesktopSessionOptions {
        clipboard: RemoteDesktopClipboardOptions {
            text: options.clipboard_text,
            images: options.clipboard_images,
            files: options.clipboard_files,
        },
        audio: RemoteDesktopAudioOptions {
            playback: options.audio_playback,
            capture: options.audio_capture,
        },
        display: RemoteDesktopDisplayOptions {
            use_all_monitors: options.use_all_monitors,
        },
        rdp: RemoteDesktopRdpOptions {
            disable_graphics_pipeline: options.disable_rdp_graphics_pipeline,
        },
        vnc: RemoteDesktopVncOptions {
            security_policy: match options.vnc_security_policy {
                PublicVncSecurityPolicy::RequireVerifiedEncryption => {
                    RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption
                }
                PublicVncSecurityPolicy::AllowUnverifiedEncryption => {
                    RemoteDesktopVncSecurityPolicy::AllowUnverifiedEncryption
                }
                PublicVncSecurityPolicy::AllowLegacy => RemoteDesktopVncSecurityPolicy::AllowLegacy,
            },
            session_mode: match options.vnc_session_mode {
                PublicVncSessionMode::Shared => RemoteDesktopVncSessionMode::Shared,
                PublicVncSessionMode::Exclusive => RemoteDesktopVncSessionMode::Exclusive,
            },
            image_quality: match options.vnc_image_quality {
                PublicVncImageQuality::Performance => RemoteDesktopVncImageQuality::Performance,
                PublicVncImageQuality::Balanced => RemoteDesktopVncImageQuality::Balanced,
                PublicVncImageQuality::BestQuality => RemoteDesktopVncImageQuality::BestQuality,
            },
            compression: match options.vnc_compression {
                PublicVncCompression::Low => RemoteDesktopVncCompression::Low,
                PublicVncCompression::Balanced => RemoteDesktopVncCompression::Balanced,
                PublicVncCompression::High => RemoteDesktopVncCompression::High,
            },
        },
    }
}

fn remove_profile(
    store: &mut oxideterm_connections::ConnectionStore,
    key: &str,
) -> Result<bool, String> {
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SSH_PREFIX) {
        return store.delete(id).map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX) {
        return store.delete_serial_profile(id).map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX) {
        return store.delete_telnet_profile(id).map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
        return store.delete_mosh_profile(id).map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX) {
        return store
            .delete_remote_desktop_profile(id)
            .map_err(public_store_error);
    }
    Err("The saved connection type is unsupported".to_owned())
}

fn store_credential(
    store: &mut oxideterm_connections::ConnectionStore,
    key: &str,
    slot: PublicCredentialSlot,
    secret: &SecretString,
) -> Result<bool, String> {
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SSH_PREFIX) {
        return store
            .store_connection_credential(id, connection_credential_slot(slot)?, secret)
            .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
        return match slot {
            PublicCredentialSlot::Primary => store.store_mosh_profile_credential(id, secret),
            PublicCredentialSlot::ProxyHop { index } => store.store_mosh_proxy_hop_credential(
                id,
                usize::try_from(index)
                    .map_err(|_| "The proxy hop index is outside the supported range")?,
                secret,
            ),
            PublicCredentialSlot::UpstreamProxy => {
                return Err("Mosh does not expose an upstream proxy credential slot".to_owned());
            }
        }
        .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX) {
        if !matches!(slot, PublicCredentialSlot::Primary) {
            return Err("Remote desktop exposes only its primary credential slot".to_owned());
        }
        return store
            .save_remote_desktop_credential(id, secret)
            .map(|_| true)
            .map_err(public_store_error);
    }
    Err("This connection type has no protected credential slots".to_owned())
}

fn forget_credential(
    store: &mut oxideterm_connections::ConnectionStore,
    key: &str,
    slot: PublicCredentialSlot,
) -> Result<bool, String> {
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SSH_PREFIX) {
        return store
            .forget_connection_credential(id, connection_credential_slot(slot)?)
            .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
        return match slot {
            PublicCredentialSlot::Primary => store.forget_mosh_profile_credential(id),
            PublicCredentialSlot::ProxyHop { index } => store.forget_mosh_proxy_hop_credential(
                id,
                usize::try_from(index)
                    .map_err(|_| "The proxy hop index is outside the supported range")?,
            ),
            PublicCredentialSlot::UpstreamProxy => {
                return Err("Mosh does not expose an upstream proxy credential slot".to_owned());
            }
        }
        .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX) {
        if !matches!(slot, PublicCredentialSlot::Primary) {
            return Err("Remote desktop exposes only its primary credential slot".to_owned());
        }
        return store
            .delete_remote_desktop_credential(id)
            .map_err(public_store_error);
    }
    Err("This connection type has no protected credential slots".to_owned())
}

fn connection_credential_slot(
    slot: PublicCredentialSlot,
) -> Result<ConnectionCredentialSlot, String> {
    match slot {
        PublicCredentialSlot::Primary => Ok(ConnectionCredentialSlot::Primary),
        PublicCredentialSlot::ProxyHop { index } => usize::try_from(index)
            .map(|index| ConnectionCredentialSlot::ProxyHop { index })
            .map_err(|_| "The proxy hop index is outside the supported range".to_owned()),
        PublicCredentialSlot::UpstreamProxy => Ok(ConnectionCredentialSlot::UpstreamProxy),
    }
}

fn credential_status(store: &oxideterm_connections::ConnectionStore, key: &str) -> Vec<Value> {
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SSH_PREFIX)
        && let Some(connection) = store.get(id)
    {
        let mut slots = vec![auth_status("primary", None, &connection.auth)];
        slots.extend(
            connection
                .proxy_chain
                .iter()
                .enumerate()
                .map(|(index, hop)| auth_status("proxy_hop", u32::try_from(index).ok(), &hop.auth)),
        );
        if let SavedUpstreamProxyPolicy::Custom { proxy } = &connection.upstream_proxy
            && let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &proxy.auth
        {
            slots.push(json!({
                "slot": { "kind": "upstream_proxy" },
                "credential_kind": "password",
                "writable": true,
                "configured": keychain_id.is_some(),
                "source": keychain_id.as_ref().map(|_| "protected_store"),
            }));
        }
        return slots;
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX)
        && let Some(profile) = store.get_mosh_profile(id)
    {
        let mut slots = vec![auth_status("primary", None, &profile.auth)];
        slots.extend(
            profile
                .proxy_chain
                .iter()
                .enumerate()
                .map(|(index, hop)| auth_status("proxy_hop", u32::try_from(index).ok(), &hop.auth)),
        );
        return slots;
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX)
        && let Some(profile) = store.get_remote_desktop_profile(id)
    {
        return vec![json!({
            "slot": { "kind": "primary" },
            "credential_kind": "password",
            "writable": true,
            "configured": profile.credential_ref.is_some(),
            "source": profile.credential_ref.as_ref().map(|_| "protected_store"),
        })];
    }
    Vec::new()
}

fn auth_status(slot_kind: &str, index: Option<u32>, auth: &SavedAuth) -> Value {
    let (credential_kind, configured, writable) = match auth.conventional_fallback() {
        SavedAuth::Password { keychain_id, .. } => ("password", keychain_id.is_some(), true),
        SavedAuth::Key {
            passphrase_keychain_id,
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id,
            ..
        }
        | SavedAuth::ManagedKey {
            passphrase_keychain_id,
            ..
        } => ("passphrase", passphrase_keychain_id.is_some(), true),
        SavedAuth::KeyboardInteractive => ("interactive", false, false),
        SavedAuth::Agent => ("agent", false, false),
        SavedAuth::KerberosPreferred { .. } => unreachable!("fallback auth is conventional"),
    };
    let slot = index.map_or_else(
        || json!({ "kind": slot_kind }),
        |index| json!({ "kind": slot_kind, "index": index }),
    );
    json!({
        "slot": slot,
        "credential_kind": credential_kind,
        "writable": writable,
        "configured": configured,
        "source": configured.then_some("protected_store"),
    })
}

fn connection_has_configured_credentials(
    store: &oxideterm_connections::ConnectionStore,
    key: &str,
) -> bool {
    credential_status(store, key)
        .iter()
        .any(|slot| slot.get("configured").and_then(Value::as_bool) == Some(true))
}

pub(super) fn connection_revision(
    store: &oxideterm_connections::ConnectionStore,
    key: &str,
) -> Option<String> {
    let timestamp = if let Some(id) = key.strip_prefix(CONNECTION_KEY_SSH_PREFIX) {
        let connection = store.get(id)?;
        connection.updated_at.unwrap_or(connection.created_at)
    } else if let Some(id) = key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX) {
        store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == id)?
            .updated_at
    } else if let Some(id) = key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX) {
        store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == id)?
            .updated_at
    } else if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
        store.get_mosh_profile(id)?.updated_at
    } else if let Some(id) = key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX) {
        store.get_remote_desktop_profile(id)?.updated_at
    } else {
        return None;
    };
    Some(timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true))
}

pub(super) fn ssh_connection_projection(
    connection_ref: &ConnectionRef,
    revision: String,
    connection: &SavedConnection,
) -> Value {
    let proxy_chain = connection
        .proxy_chain
        .iter()
        .map(|hop| {
            json!({
                "host": hop.host,
                "port": hop.port,
                "username": hop.username,
                "auth": auth_projection(&hop.auth),
                "agent_forwarding": hop.agent_forwarding,
                "identity_agent": hop.identity_agent,
                "agent_forwarding_socket": hop.agent_forwarding_socket,
                "legacy_ssh_compatibility": hop.legacy_ssh_compatibility,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "connection_ref": connection_ref,
        "revision": revision,
        "type": "ssh",
        "name": connection.name,
        "group": connection.group,
        "notes": connection.notes,
        "host": connection.host,
        "port": connection.port,
        "username": connection.username,
        "auth": auth_projection(&connection.auth),
        "proxy_chain": proxy_chain,
        "upstream_proxy": upstream_proxy_projection(&connection.upstream_proxy),
        "color": connection.color,
        "icon_background_color": connection.icon_background_color,
        "icon": connection.icon,
        "tags": connection.tags,
        "connect_timeout_seconds": connection.options.effective_connect_timeout_seconds(),
        "agent_forwarding": connection.options.agent_forwarding,
        "identity_agent": connection.options.identity_agent,
        "agent_forwarding_socket": connection.options.agent_forwarding_socket,
        "legacy_ssh_compatibility": connection.options.legacy_ssh_compatibility,
        "dedicated_new_terminal_connection": connection.options.dedicated_new_terminal_connection,
        "x11_forwarding": {
            "enabled": connection.options.x11_forwarding.enabled,
            "mode": match connection.options.x11_forwarding.mode {
                ConnectionX11ForwardingMode::Untrusted => "untrusted",
                ConnectionX11ForwardingMode::Trusted => "trusted",
            },
            "untrusted_timeout_seconds": connection.options.x11_forwarding.untrusted_timeout_seconds,
        },
        "post_connect_command": connection.post_connect_command(),
        "terminal": terminal_options_projection(&connection.options.terminal),
        "last_used_at": connection.last_used_at.map(|time| time.to_rfc3339()),
    })
}

pub(super) fn auth_projection(auth: &SavedAuth) -> Value {
    if let SavedAuth::KerberosPreferred {
        server_identity,
        delegate_credentials,
        fallback,
    } = auth
    {
        return json!({
            "kind": "kerberos_preferred",
            "server_identity": server_identity,
            "delegate_credentials": delegate_credentials,
            "fallback": auth_projection(fallback),
        });
    }
    match auth {
        SavedAuth::Password { keychain_id, .. } => json!({
            "kind": "password",
            "credential_configured": keychain_id.is_some(),
        }),
        SavedAuth::Key {
            key_path,
            passphrase_keychain_id,
            ..
        } => json!({
            "kind": "key",
            "key_path": key_path,
            "credential_configured": passphrase_keychain_id.is_some(),
        }),
        SavedAuth::ManagedKey {
            key_id,
            passphrase_keychain_id,
            ..
        } => json!({
            "kind": "managed_key",
            "managed_key_id": key_id,
            "credential_configured": passphrase_keychain_id.is_some(),
        }),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            passphrase_keychain_id,
            ..
        } => json!({
            "kind": "certificate",
            "key_path": key_path,
            "certificate_path": cert_path,
            "credential_configured": passphrase_keychain_id.is_some(),
        }),
        SavedAuth::KeyboardInteractive => json!({
            "kind": "keyboard_interactive",
            "credential_configured": false,
        }),
        SavedAuth::Agent => json!({
            "kind": "agent",
            "credential_configured": false,
        }),
        SavedAuth::KerberosPreferred { .. } => unreachable!("handled above"),
    }
}

pub(super) fn remote_desktop_options_projection(options: &RemoteDesktopSessionOptions) -> Value {
    json!({
        "clipboard_text": options.clipboard.text,
        "clipboard_images": options.clipboard.images,
        "clipboard_files": options.clipboard.files,
        "audio_playback": options.audio.playback,
        "audio_capture": options.audio.capture,
        "use_all_monitors": options.display.use_all_monitors,
        "disable_rdp_graphics_pipeline": options.rdp.disable_graphics_pipeline,
        "vnc_security_policy": match options.vnc.security_policy {
            RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption => "require_verified_encryption",
            RemoteDesktopVncSecurityPolicy::AllowUnverifiedEncryption => "allow_unverified_encryption",
            RemoteDesktopVncSecurityPolicy::AllowLegacy => "allow_legacy",
        },
        "vnc_session_mode": match options.vnc.session_mode {
            RemoteDesktopVncSessionMode::Shared => "shared",
            RemoteDesktopVncSessionMode::Exclusive => "exclusive",
        },
        "vnc_image_quality": match options.vnc.image_quality {
            RemoteDesktopVncImageQuality::Performance => "performance",
            RemoteDesktopVncImageQuality::Balanced => "balanced",
            RemoteDesktopVncImageQuality::BestQuality => "best_quality",
        },
        "vnc_compression": match options.vnc.compression {
            RemoteDesktopVncCompression::Low => "low",
            RemoteDesktopVncCompression::Balanced => "balanced",
            RemoteDesktopVncCompression::High => "high",
        },
    })
}

fn upstream_proxy_projection(policy: &SavedUpstreamProxyPolicy) -> Value {
    match policy {
        SavedUpstreamProxyPolicy::UseGlobal => json!({ "mode": "use_global" }),
        SavedUpstreamProxyPolicy::Direct => json!({ "mode": "direct" }),
        SavedUpstreamProxyPolicy::Custom { proxy } => {
            let (username, credential_configured) = match &proxy.auth {
                SavedUpstreamProxyAuth::None => (None, false),
                SavedUpstreamProxyAuth::Password {
                    username,
                    keychain_id,
                    ..
                } => (Some(username.as_str()), keychain_id.is_some()),
            };
            json!({
                "mode": "custom",
                "protocol": match proxy.protocol {
                    SavedUpstreamProxyProtocol::Socks5 => "socks5",
                    SavedUpstreamProxyProtocol::HttpConnect => "http_connect",
                },
                "host": proxy.host,
                "port": proxy.port,
                "username": username,
                "credential_configured": credential_configured,
                "remote_dns": proxy.remote_dns,
                "no_proxy": proxy.no_proxy,
            })
        }
    }
}

pub(super) fn terminal_options_projection(options: &ConnectionTerminalOptions) -> Value {
    json!({
        "encoding": options.encoding.map(|encoding| match encoding {
            ConnectionTerminalEncoding::Utf8 => "utf8",
            ConnectionTerminalEncoding::Gbk => "gbk",
            ConnectionTerminalEncoding::Gb18030 => "gb18030",
            ConnectionTerminalEncoding::Big5 => "big5",
            ConnectionTerminalEncoding::ShiftJis => "shift_jis",
            ConnectionTerminalEncoding::EucJp => "euc_jp",
            ConnectionTerminalEncoding::EucKr => "euc_kr",
            ConnectionTerminalEncoding::Windows1252 => "windows1252",
        }),
        "backspace_sequence": options.backspace_sequence.map(|sequence| match sequence {
            ConnectionTerminalBackspaceSequence::Delete => "delete",
            ConnectionTerminalBackspaceSequence::ControlH => "control_h",
        }),
        "delete_sequence": options.delete_sequence.map(|sequence| match sequence {
            ConnectionTerminalDeleteSequence::Csi3Tilde => "csi3_tilde",
            ConnectionTerminalDeleteSequence::Delete => "delete",
            ConnectionTerminalDeleteSequence::ControlH => "control_h",
        }),
        "semantic_scheme": options.semantic_scheme,
    })
}

fn required_text<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} is required for the selected authentication method"))
}

fn public_store_error(error: anyhow::Error) -> String {
    let message = error.to_string();
    // Store errors are user-facing validation or availability failures; secret values never enter them.
    if message.is_empty() {
        "The saved connection operation failed".to_owned()
    } else {
        message
    }
}
