// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_connections::ConnectionStore;

pub(super) fn detect_ssh_agent_available(identity_agent: &str) -> Option<bool> {
    oxideterm_ssh::ssh_agent_available(identity_agent_selector(identity_agent))
}

pub(super) fn proxy_chain_from_form(
    form: &mut NewConnectionForm,
    secret_handoff: RuntimeSecretHandoff,
    saved_auth: Vec<Option<AuthMethod>>,
) -> Option<Vec<ProxyHopConfig>> {
    if form.proxy_hops.is_empty() {
        return None;
    }

    let mut chain = Vec::new();
    let mut saved_auth = saved_auth.into_iter();
    for hop in form.proxy_hops.iter_mut().filter(|hop| hop.complete()) {
        chain.push(ProxyHopConfig {
            host: hop.host.trim().to_string(),
            port: hop.port.trim().parse::<u16>().unwrap_or(22),
            username: hop.username.trim().to_string(),
            auth: saved_auth
                .next()
                .flatten()
                .unwrap_or_else(|| auth_method_from_proxy_hop(hop, secret_handoff)),
            agent_forwarding: hop.agent_forwarding,
            identity_agent: identity_agent_from_form(&hop.identity_agent),
            agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
            legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
            ssh_algorithms: hop.ssh_algorithms.clone(),
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });
    }
    debug_assert!(saved_auth.next().is_none());

    Some(chain)
}

pub(super) fn saved_proxy_hop_auth_from_store(
    connection_store: &ConnectionStore,
    form: &NewConnectionForm,
    missing_credentials_message: &str,
) -> Result<Vec<Option<AuthMethod>>, String> {
    form.proxy_hops
        .iter()
        .filter(|hop| hop.complete())
        .map(|hop| {
            if hop.saved_connection_id.is_empty() || hop.has_explicit_secret_draft() {
                return Ok(None);
            }
            let saved_connection = connection_store
                .get(&hop.saved_connection_id)
                .ok_or_else(|| missing_credentials_message.to_string())?;
            if !hop.matches_saved_connection(saved_connection) {
                return Err(missing_credentials_message.to_string());
            }
            // Resolve directly into a zeroizing runtime owner without exposing the secret
            // through the metadata-only jump-server form.
            auth_method_from_saved_auth(connection_store, &saved_connection.auth)
                .map(Some)
                .ok_or_else(|| missing_credentials_message.to_string())
        })
        .collect()
}

pub(super) fn validate_proxy_chain_form(form: &NewConnectionForm) -> Result<(), String> {
    for hop in form.proxy_hops.iter().filter(|hop| hop.complete()) {
        if hop.auth_tab == SshAuthTab::ManagedKey && hop.managed_key_id.trim().is_empty() {
            return Err("Proxy hop managed key is required".to_string());
        }
        if matches!(hop.auth_tab, SshAuthTab::SshKey | SshAuthTab::Certificate)
            && hop.key_path.trim().is_empty()
        {
            return Err("Proxy hop key path is required".to_string());
        }
        if hop.auth_tab == SshAuthTab::Certificate && hop.cert_path.trim().is_empty() {
            return Err("Proxy hop certificate path is required".to_string());
        }
    }
    Ok(())
}

pub(super) fn proxy_session_tree_endpoints(
    config: &SshConfig,
) -> Vec<NativeSessionTreeConnectEndpoint> {
    let mut endpoints = config
        .proxy_chain
        .as_ref()
        .map(|chain| {
            chain
                .iter()
                .map(|hop| NativeSessionTreeConnectEndpoint::new(hop.host.clone(), hop.port))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    endpoints.push(NativeSessionTreeConnectEndpoint::new(
        config.host.clone(),
        config.port,
    ));
    endpoints
}

pub(super) fn prepare_proxy_chain_test_config(config: &mut SshConfig) {
    config.strict_host_key_checking = true;
    config.trust_host_key = Some(false);
    config.expected_host_key_fingerprint = None;

    if let Some(chain) = config.proxy_chain.as_mut() {
        for hop in chain {
            hop.strict_host_key_checking = true;
            hop.trust_host_key = Some(false);
            hop.expected_host_key_fingerprint = None;
        }
    }
}

pub(super) fn prepare_tree_connect_config(config: &mut SshConfig) -> Result<(), String> {
    // Tauri resolves `default_key` to the first existing default key before
    // adding/connecting SessionTree nodes, while test_connection keeps its own
    // dynamic loader. Native mirrors that split here.
    resolve_default_key_for_tree_auth(&mut config.auth)?;
    if let Some(chain) = config.proxy_chain.as_mut() {
        for hop in chain {
            resolve_default_key_for_tree_auth(&mut hop.auth)?;
        }
    }
    Ok(())
}

pub(super) fn resolve_default_key_for_tree_auth(auth: &mut AuthMethod) -> Result<(), String> {
    match auth {
        AuthMethod::KerberosPreferred { fallback, .. } => {
            resolve_default_key_for_tree_auth(fallback)
        }
        AuthMethod::Key { key_path, .. } if key_path.trim().is_empty() => {
            *key_path = first_available_default_key_path().map_err(|error| error.to_string())?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn auth_method_from_proxy_hop(
    hop: &mut NewConnectionProxyHop,
    secret_handoff: RuntimeSecretHandoff,
) -> AuthMethod {
    let fallback = match hop.auth_tab {
        SshAuthTab::Password => {
            AuthMethod::password_secret(secret_handoff.zeroizing(&mut hop.password))
        }
        SshAuthTab::DefaultKey => {
            AuthMethod::key_secret("", secret_handoff.zeroizing_non_empty(&mut hop.passphrase))
        }
        SshAuthTab::SshKey => AuthMethod::key_secret(
            hop.key_path.trim().to_string(),
            secret_handoff.zeroizing_non_empty(&mut hop.passphrase),
        ),
        SshAuthTab::ManagedKey => AuthMethod::managed_key_secret(
            hop.managed_key_id.trim().to_string(),
            secret_handoff.zeroizing_non_empty(&mut hop.passphrase),
        ),
        SshAuthTab::Certificate => AuthMethod::certificate_secret(
            hop.key_path.trim().to_string(),
            hop.cert_path.trim().to_string(),
            secret_handoff.zeroizing_non_empty(&mut hop.passphrase),
        ),
        SshAuthTab::Agent => AuthMethod::Agent,
        SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
    };
    if hop.gssapi_enabled {
        AuthMethod::kerberos_preferred(
            fallback,
            (!hop.gssapi_server_identity.trim().is_empty())
                .then(|| hop.gssapi_server_identity.trim().to_string()),
            hop.gssapi_delegate_credentials,
        )
    } else {
        fallback
    }
}

pub(super) fn form_from_runtime_config(
    config: SshConfig,
    title: Option<&str>,
    default_group: String,
) -> NewConnectionForm {
    let auth_fields = runtime_auth_form_fields(config.auth);
    let mut form = NewConnectionForm::default();
    form.name = title
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}@{}", config.username, config.host));
    form.host = config.host.clone();
    form.port = config.port.to_string();
    form.username = config.username.clone();
    form.auth_tab = auth_fields.auth_tab;
    form.password = auth_fields.password;
    form.key_path = auth_fields.key_path;
    form.managed_key_id = auth_fields.managed_key_id;
    form.cert_path = auth_fields.cert_path;
    form.passphrase = auth_fields.passphrase;
    form.gssapi_enabled = auth_fields.gssapi_enabled;
    form.gssapi_server_identity = auth_fields.gssapi_server_identity;
    form.gssapi_delegate_credentials = auth_fields.gssapi_delegate_credentials;
    form.group = default_group;
    form.post_connect_command = config.post_connect_command.clone().unwrap_or_default();
    form.agent_forwarding = config.agent_forwarding;
    form.identity_agent = config.identity_agent.clone().unwrap_or_default();
    form.agent_forwarding_socket = config.agent_forwarding_socket.clone();
    form.legacy_ssh_compatibility = config.legacy_ssh_compatibility;
    form.connect_timeout_seconds = config.timeout_secs;
    form.connect_timeout_seconds_text = config.timeout_secs.to_string();
    form.x11_forwarding = connection_x11_options(config.x11_forwarding);
    form.save_password = auth_fields.save_password;

    if let Some(chain) = config.proxy_chain {
        form.proxy_hops = chain
            .into_iter()
            .map(proxy_hop_form_from_runtime_config)
            .collect();
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    }
    if let Some(proxy) = config.upstream_proxy {
        form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
        form.upstream_proxy_protocol = match proxy.protocol {
            UpstreamProxyProtocol::Socks5 => SavedUpstreamProxyProtocol::Socks5,
            UpstreamProxyProtocol::HttpConnect => SavedUpstreamProxyProtocol::HttpConnect,
        };
        form.upstream_proxy_host = proxy.host.clone();
        form.upstream_proxy_port = proxy.port.to_string();
        form.upstream_proxy_remote_dns = proxy.remote_dns;
        form.upstream_proxy_no_proxy = proxy.no_proxy.clone();
        if let UpstreamProxyAuth::Password {
            username,
            mut password,
        } = proxy.auth
        {
            form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
            form.upstream_proxy_username = username;
            form.upstream_proxy_password = std::mem::take(&mut *password);
        }
    }
    form
}

pub(super) fn proxy_hop_form_from_runtime_config(config: ProxyHopConfig) -> NewConnectionProxyHop {
    let auth_fields = runtime_auth_form_fields(config.auth);
    NewConnectionProxyHop {
        saved_connection_id: String::new(),
        persisted_proxy_hop_index: None,
        host: config.host,
        port: config.port.to_string(),
        username: config.username,
        auth_tab: auth_fields.auth_tab,
        key_path: auth_fields.key_path,
        managed_key_id: auth_fields.managed_key_id,
        cert_path: auth_fields.cert_path,
        // Dynamic drill-down save-as must persist a usable proxy chain. Runtime
        // secrets are copied only after the user explicitly asks to save this
        // live path; the connection store then moves them into the keychain.
        password: auth_fields.password,
        passphrase: auth_fields.passphrase,
        gssapi_enabled: auth_fields.gssapi_enabled,
        gssapi_server_identity: auth_fields.gssapi_server_identity,
        gssapi_delegate_credentials: auth_fields.gssapi_delegate_credentials,
        agent_forwarding: config.agent_forwarding,
        identity_agent: config.identity_agent.unwrap_or_default(),
        agent_forwarding_socket: config.agent_forwarding_socket,
        legacy_ssh_compatibility: config.legacy_ssh_compatibility,
        ssh_algorithms: config.ssh_algorithms,
    }
}

struct RuntimeAuthFormFields {
    auth_tab: SshAuthTab,
    password: String,
    key_path: String,
    managed_key_id: String,
    cert_path: String,
    passphrase: String,
    save_password: bool,
    gssapi_enabled: bool,
    gssapi_server_identity: String,
    gssapi_delegate_credentials: bool,
}

fn runtime_auth_form_fields(auth: AuthMethod) -> RuntimeAuthFormFields {
    match auth {
        AuthMethod::Password { mut password } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Password,
            password: std::mem::take(&mut *password),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: true,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::Key {
            key_path,
            mut passphrase,
        } if key_path.trim().is_empty() => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::DefaultKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_mut()
                .map(|value| std::mem::take(&mut **value))
                .unwrap_or_default(),
            save_password: false,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::Key {
            key_path,
            mut passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
            key_path,
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_mut()
                .map(|value| std::mem::take(&mut **value))
                .unwrap_or_default(),
            save_password: false,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::ManagedKey {
            key_id,
            mut passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::ManagedKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: key_id.clone(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_mut()
                .map(|value| std::mem::take(&mut **value))
                .unwrap_or_default(),
            save_password: false,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::Certificate {
            key_path,
            cert_path,
            mut passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Certificate,
            password: String::new(),
            key_path: key_path.clone(),
            managed_key_id: String::new(),
            cert_path: cert_path.clone(),
            passphrase: passphrase
                .as_mut()
                .map(|value| std::mem::take(&mut **value))
                .unwrap_or_default(),
            save_password: false,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::Agent => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Agent,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::KeyboardInteractive => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::TwoFactor,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
        },
        AuthMethod::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback,
        } => {
            let mut fields = runtime_auth_form_fields(*fallback);
            fields.gssapi_enabled = true;
            fields.gssapi_server_identity = server_identity.unwrap_or_default();
            fields.gssapi_delegate_credentials = delegate_credentials;
            fields
        }
    }
}

#[cfg(test)]
mod runtime_save_tests {
    use std::io::Write;

    use super::*;
    use tempfile::NamedTempFile;
    use zeroize::Zeroizing;

    #[test]
    fn test_secret_handoff_keeps_the_form_reusable() {
        let mut form_secret = "target-secret".to_string();

        let runtime_secret = RuntimeSecretHandoff::CopyForTest.zeroizing(&mut form_secret);

        assert_eq!(runtime_secret.as_str(), "target-secret");
        assert_eq!(form_secret, "target-secret");
    }

    #[test]
    fn connection_secret_handoff_moves_the_form_allocation() {
        let mut form_secret = "target-secret".to_string();
        let form_secret_pointer = form_secret.as_ptr();

        let runtime_secret = RuntimeSecretHandoff::Move.zeroizing(&mut form_secret);

        assert_eq!(runtime_secret.as_str(), "target-secret");
        assert_eq!(runtime_secret.as_ptr(), form_secret_pointer);
        assert!(form_secret.is_empty());
    }

    #[test]
    fn proxy_test_secret_handoff_keeps_the_hop_reusable() {
        let mut hop = NewConnectionProxyHop::new();
        hop.auth_tab = SshAuthTab::Password;
        hop.password = "jump-secret".to_string();

        let auth = auth_method_from_proxy_hop(&mut hop, RuntimeSecretHandoff::CopyForTest);

        assert!(matches!(
            auth,
            AuthMethod::Password { ref password } if password.as_str() == "jump-secret"
        ));
        assert_eq!(hop.password, "jump-secret");
    }

    #[test]
    fn two_saved_password_hops_hydrate_without_populating_the_form() {
        let mut store_file = NamedTempFile::new().unwrap();
        // Read-only legacy data exercises the runtime handoff without requiring
        // an operating-system credential service in the test environment.
        let fixture = Zeroizing::new(
            r#"{
              "connections": [
                {
                  "id": "public-proxy",
                  "name": "public-proxy",
                  "host": "proxy.example.com",
                  "port": 22,
                  "username": "proxy-user",
                  "auth": { "type": "password", "password": "public-proxy-secret" },
                  "created_at": "2026-01-01T00:00:00Z"
                },
                {
                  "id": "gateway",
                  "name": "gateway",
                  "host": "gateway.internal",
                  "port": 22,
                  "username": "gateway-user",
                  "auth": { "type": "password", "password": "gateway-secret" },
                  "created_at": "2026-01-01T00:00:00Z"
                }
              ],
              "groups": []
            }"#
            .to_string(),
        );
        store_file.write_all(fixture.as_bytes()).unwrap();
        store_file.flush().unwrap();
        let connection_store = ConnectionStore::load_read_only(store_file.path()).unwrap();
        let connection_infos = connection_store.connection_infos();
        let public_proxy = connection_infos
            .iter()
            .find(|connection| connection.id == "public-proxy")
            .unwrap();
        let gateway = connection_infos
            .iter()
            .find(|connection| connection.id == "gateway")
            .unwrap();
        let mut form = NewConnectionForm::default();
        for connection in [public_proxy, gateway] {
            let mut hop = NewConnectionProxyHop::new();
            hop.apply_saved_connection(connection);
            form.proxy_hops.push(hop);
        }

        let saved_auth =
            saved_proxy_hop_auth_from_store(&connection_store, &form, "missing saved credentials")
                .unwrap();
        let proxy_chain =
            proxy_chain_from_form(&mut form, RuntimeSecretHandoff::CopyForTest, saved_auth)
                .expect("proxy chain");

        assert!(matches!(
            &proxy_chain[0].auth,
            AuthMethod::Password { password } if password.as_str() == "public-proxy-secret"
        ));
        assert!(matches!(
            &proxy_chain[1].auth,
            AuthMethod::Password { password } if password.as_str() == "gateway-secret"
        ));
        assert!(form.proxy_hops.iter().all(|hop| hop.password.is_empty()));
        let debug_output = format!("{proxy_chain:?}");
        assert!(!debug_output.contains("public-proxy-secret"));
        assert!(!debug_output.contains("gateway-secret"));
    }

    #[test]
    fn runtime_proxy_hop_form_preserves_password_for_save_as() {
        let hop = proxy_hop_form_from_runtime_config(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: AuthMethod::password_secret(Zeroizing::new("jump-secret".to_string())),
            agent_forwarding: true,
            identity_agent: Some("/tmp/jump-agent.sock".to_string()),
            agent_forwarding_socket: Some("/tmp/jump-forward.sock".to_string()),
            legacy_ssh_compatibility: true,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });

        assert_eq!(hop.auth_tab, SshAuthTab::Password);
        assert_eq!(hop.password, "jump-secret");
        assert!(hop.agent_forwarding);
        assert_eq!(hop.identity_agent, "/tmp/jump-agent.sock");
        assert_eq!(
            hop.agent_forwarding_socket.as_deref(),
            Some("/tmp/jump-forward.sock")
        );
        assert!(hop.legacy_ssh_compatibility);
    }

    #[test]
    fn runtime_proxy_hop_form_preserves_key_passphrase_for_save_as() {
        let hop = proxy_hop_form_from_runtime_config(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: AuthMethod::key_secret(
                "/home/ops/.ssh/id_ed25519",
                Some(Zeroizing::new("key-secret".to_string())),
            ),
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });

        assert_eq!(hop.auth_tab, SshAuthTab::SshKey);
        assert_eq!(hop.key_path, "/home/ops/.ssh/id_ed25519");
        assert_eq!(hop.passphrase, "key-secret");
    }

    #[test]
    fn runtime_target_form_marks_password_for_persistence() {
        let form = form_from_runtime_config(
            SshConfig {
                host: "target.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth: AuthMethod::password_secret(Zeroizing::new("target-secret".to_string())),
                identity_agent: Some("/tmp/target-agent.sock".to_string()),
                agent_forwarding_socket: Some("/tmp/target-forward.sock".to_string()),
                ..SshConfig::default()
            },
            None,
            "Ungrouped".to_string(),
        );

        assert_eq!(form.auth_tab, SshAuthTab::Password);
        assert_eq!(form.password, "target-secret");
        assert!(form.save_password);
        assert_eq!(form.identity_agent, "/tmp/target-agent.sock");
        assert_eq!(
            form.agent_forwarding_socket.as_deref(),
            Some("/tmp/target-forward.sock")
        );
    }

    #[test]
    fn runtime_form_preserves_upstream_proxy_password_for_save_as() {
        let form = form_from_runtime_config(
            SshConfig {
                host: "target.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth: AuthMethod::Agent,
                upstream_proxy: Some(oxideterm_ssh::UpstreamProxyConfig {
                    protocol: UpstreamProxyProtocol::Socks5,
                    host: "127.0.0.1".to_string(),
                    port: 1080,
                    auth: UpstreamProxyAuth::Password {
                        username: "proxy-user".to_string(),
                        password: Zeroizing::new("proxy-secret".to_string()),
                    },
                    remote_dns: true,
                    no_proxy: String::new(),
                }),
                ..SshConfig::default()
            },
            None,
            "Ungrouped".to_string(),
        );

        assert_eq!(
            form.upstream_proxy_auth,
            NewConnectionUpstreamProxyAuth::Password
        );
        assert_eq!(form.upstream_proxy_username, "proxy-user");
        assert_eq!(form.upstream_proxy_password, "proxy-secret");
    }

    #[test]
    fn saved_connection_title_sync_updates_only_matching_nodes() {
        let mut nodes = HashMap::from([
            (
                NodeId::new("node-home"),
                WorkspaceSshNode::new(
                    Some("home".to_string()),
                    &SshConfig {
                        host: "100.118.61.75".to_string(),
                        ..SshConfig::default()
                    },
                    "Old Home".to_string(),
                    Vec::new(),
                    NodeReadiness::Ready,
                ),
            ),
            (
                NodeId::new("node-prod"),
                WorkspaceSshNode::new(
                    Some("prod".to_string()),
                    &SshConfig {
                        host: "prod.example.com".to_string(),
                        ..SshConfig::default()
                    },
                    "Production".to_string(),
                    Vec::new(),
                    NodeReadiness::Ready,
                ),
            ),
        ]);

        assert!(sync_saved_connection_node_title_for_nodes(
            &mut nodes,
            "home",
            "Renamed Home"
        ));

        let home = nodes.get(&NodeId::new("node-home")).unwrap();
        let prod = nodes.get(&NodeId::new("node-prod")).unwrap();
        assert_eq!(home.title, "Renamed Home");
        assert_eq!(home.endpoint.host, "100.118.61.75");
        assert_eq!(prod.title, "Production");
    }
}

pub(super) fn serial_profile_name_or_port(name: &str, port_path: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        port_path.to_string()
    } else {
        name.to_string()
    }
}

pub(super) fn telnet_profile_name_or_endpoint(name: &str, host: &str, port: u16) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!("{}:{}", host.trim(), port)
    } else {
        name.to_string()
    }
}

pub(super) fn remote_desktop_protocol_for_transport(
    transport: NewConnectionTransport,
) -> Option<RemoteDesktopProtocol> {
    match transport {
        NewConnectionTransport::Rdp => Some(RemoteDesktopProtocol::Rdp),
        NewConnectionTransport::Vnc => Some(RemoteDesktopProtocol::Vnc),
        _ => None,
    }
}

pub(super) fn remote_desktop_profile_label(
    name: &str,
    protocol: RemoteDesktopProtocol,
    host: &str,
    port: u16,
) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!(
            "{}://{}:{port}",
            protocol.provider_id(),
            remote_desktop_label_host(host)
        )
    } else {
        name.to_string()
    }
}

pub(super) fn remote_desktop_label_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        // Keep IPv6 endpoint labels parseable when shown in tab titles.
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

pub(super) fn serial_profile_group_from_form(
    group: &str,
    i18n: &oxideterm_i18n::I18n,
) -> Option<String> {
    let group = group.trim();
    if group.is_empty()
        || group == "Ungrouped"
        || group == "未分组"
        || group == i18n.t("ssh.form.ungrouped")
        || group == i18n.t("sessionManager.edit_properties.ungrouped")
    {
        None
    } else {
        Some(group.to_string())
    }
}

pub(super) fn asset_icon_from_form(icon: &str) -> Option<String> {
    // Empty means the asset should use its transport-specific fallback icon.
    let icon = icon.trim();
    (!icon.is_empty()).then(|| icon.to_string())
}

pub(super) fn asset_color_from_form(color: &str) -> Option<String> {
    // Empty means the asset should use its transport-specific fallback color.
    let color = color.trim();
    (!color.is_empty()).then(|| color.to_string())
}

pub(super) fn serial_profile_parity_from_terminal(
    parity: oxideterm_terminal::SerialParity,
) -> oxideterm_connections::SerialParity {
    match parity {
        oxideterm_terminal::SerialParity::None => oxideterm_connections::SerialParity::None,
        oxideterm_terminal::SerialParity::Odd => oxideterm_connections::SerialParity::Odd,
        oxideterm_terminal::SerialParity::Even => oxideterm_connections::SerialParity::Even,
    }
}

pub(super) fn serial_profile_flow_from_terminal(
    flow: oxideterm_terminal::SerialFlowControl,
) -> oxideterm_connections::SerialFlowControl {
    match flow {
        oxideterm_terminal::SerialFlowControl::None => {
            oxideterm_connections::SerialFlowControl::None
        }
        oxideterm_terminal::SerialFlowControl::Software => {
            oxideterm_connections::SerialFlowControl::Software
        }
        oxideterm_terminal::SerialFlowControl::Hardware => {
            oxideterm_connections::SerialFlowControl::Hardware
        }
    }
}

pub(super) fn take_zeroizing_secret(value: &mut String) -> zeroize::Zeroizing<String> {
    // Preserve the UI allocation while transferring it to the runtime secret owner.
    zeroize::Zeroizing::new(std::mem::take(value))
}
