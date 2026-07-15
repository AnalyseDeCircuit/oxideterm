// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn detect_ssh_agent_available() -> Option<bool> {
    let sock = std::env::var_os("SSH_AUTH_SOCK")?;
    Some(!sock.is_empty() && std::path::Path::new(&sock).exists())
}

pub(super) fn proxy_chain_from_form(
    form: &mut NewConnectionForm,
) -> Result<Option<Vec<ProxyHopConfig>>, String> {
    if form.proxy_hops.is_empty() {
        return Ok(None);
    }

    let mut chain = Vec::new();
    for hop in form.proxy_hops.iter().filter(|hop| hop.complete()) {
        if hop.auth_tab == SshAuthTab::ManagedKey && hop.managed_key_id.trim().is_empty() {
            return Err("Proxy hop managed key is required".to_string());
        }
        chain.push(ProxyHopConfig {
            host: hop.host.trim().to_string(),
            port: hop.port.trim().parse::<u16>().unwrap_or(22),
            username: hop.username.trim().to_string(),
            auth: auth_method_from_proxy_hop(hop),
            agent_forwarding: hop.agent_forwarding,
            legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });
    }

    Ok(Some(chain))
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
        AuthMethod::Key { key_path, .. } if key_path.trim().is_empty() => {
            *key_path = first_available_default_key_path().map_err(|error| error.to_string())?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn auth_method_from_proxy_hop(hop: &NewConnectionProxyHop) -> AuthMethod {
    match hop.auth_tab {
        SshAuthTab::Password => AuthMethod::password_secret(zeroizing_secret_clone(&hop.password)),
        SshAuthTab::DefaultKey => {
            AuthMethod::key_secret("", zeroizing_non_empty_secret(&hop.passphrase))
        }
        SshAuthTab::SshKey => AuthMethod::key_secret(
            hop.key_path.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::ManagedKey => AuthMethod::managed_key_secret(
            hop.managed_key_id.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::Certificate => AuthMethod::certificate_secret(
            hop.key_path.trim().to_string(),
            hop.cert_path.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::Agent => AuthMethod::Agent,
        SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
    }
}

pub(super) fn form_from_runtime_config(
    config: &SshConfig,
    title: Option<&str>,
    default_group: String,
) -> NewConnectionForm {
    let auth_fields = runtime_auth_form_fields(&config.auth);
    let mut form = NewConnectionForm {
        name: title
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}@{}", config.username, config.host)),
        host: config.host.clone(),
        port: config.port.to_string(),
        username: config.username.clone(),
        auth_tab: auth_fields.auth_tab,
        password: auth_fields.password,
        key_path: auth_fields.key_path,
        managed_key_id: auth_fields.managed_key_id,
        cert_path: auth_fields.cert_path,
        passphrase: auth_fields.passphrase,
        group: default_group,
        post_connect_command: config.post_connect_command.clone().unwrap_or_default(),
        agent_forwarding: config.agent_forwarding,
        legacy_ssh_compatibility: config.legacy_ssh_compatibility,
        save_password: auth_fields.save_password,
        ..NewConnectionForm::default()
    };

    if let Some(chain) = &config.proxy_chain {
        form.proxy_hops = chain
            .iter()
            .cloned()
            .map(proxy_hop_form_from_runtime_config)
            .collect();
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    }
    if let Some(proxy) = &config.upstream_proxy {
        form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
        form.upstream_proxy_protocol = match proxy.protocol {
            UpstreamProxyProtocol::Socks5 => SavedUpstreamProxyProtocol::Socks5,
            UpstreamProxyProtocol::HttpConnect => SavedUpstreamProxyProtocol::HttpConnect,
        };
        form.upstream_proxy_host = proxy.host.clone();
        form.upstream_proxy_port = proxy.port.to_string();
        form.upstream_proxy_remote_dns = proxy.remote_dns;
        form.upstream_proxy_no_proxy = proxy.no_proxy.clone();
        if let UpstreamProxyAuth::Password { username, password } = &proxy.auth {
            form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
            form.upstream_proxy_username = username.clone();
            form.upstream_proxy_password = password.as_str().to_string();
        }
    }
    form
}

pub(super) fn proxy_hop_form_from_runtime_config(config: ProxyHopConfig) -> NewConnectionProxyHop {
    let auth_fields = runtime_auth_form_fields(&config.auth);
    NewConnectionProxyHop {
        saved_connection_id: String::new(),
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
        agent_forwarding: config.agent_forwarding,
        legacy_ssh_compatibility: config.legacy_ssh_compatibility,
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
}

fn runtime_auth_form_fields(auth: &AuthMethod) -> RuntimeAuthFormFields {
    match auth {
        AuthMethod::Password { password } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Password,
            password: password.as_str().to_string(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: true,
        },
        AuthMethod::Key {
            key_path,
            passphrase,
        } if key_path.trim().is_empty() => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::DefaultKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::Key {
            key_path,
            passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
            key_path: key_path.clone(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::ManagedKey { key_id, passphrase } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::ManagedKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: key_id.clone(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Certificate,
            password: String::new(),
            key_path: key_path.clone(),
            managed_key_id: String::new(),
            cert_path: cert_path.clone(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::Agent => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Agent,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
        },
        AuthMethod::KeyboardInteractive => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::TwoFactor,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
        },
    }
}

#[cfg(test)]
mod runtime_save_tests {
    use super::*;
    use zeroize::Zeroizing;

    #[test]
    fn runtime_proxy_hop_form_preserves_password_for_save_as() {
        let hop = proxy_hop_form_from_runtime_config(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: AuthMethod::password_secret(Zeroizing::new("jump-secret".to_string())),
            agent_forwarding: true,
            legacy_ssh_compatibility: true,
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });

        assert_eq!(hop.auth_tab, SshAuthTab::Password);
        assert_eq!(hop.password, "jump-secret");
        assert!(hop.agent_forwarding);
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
            legacy_ssh_compatibility: false,
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
            &SshConfig {
                host: "target.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth: AuthMethod::password_secret(Zeroizing::new("target-secret".to_string())),
                ..SshConfig::default()
            },
            None,
            "Ungrouped".to_string(),
        );

        assert_eq!(form.auth_tab, SshAuthTab::Password);
        assert_eq!(form.password, "target-secret");
        assert!(form.save_password);
    }

    #[test]
    fn runtime_form_preserves_upstream_proxy_password_for_save_as() {
        let form = form_from_runtime_config(
            &SshConfig {
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
                WorkspaceSshNode {
                    saved_connection_id: Some("home".to_string()),
                    config: SshConfig {
                        host: "100.118.61.75".to_string(),
                        ..SshConfig::default()
                    },
                    title: "Old Home".to_string(),
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Ready,
                },
            ),
            (
                NodeId::new("node-prod"),
                WorkspaceSshNode {
                    saved_connection_id: Some("prod".to_string()),
                    config: SshConfig {
                        host: "prod.example.com".to_string(),
                        ..SshConfig::default()
                    },
                    title: "Production".to_string(),
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Ready,
                },
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
        assert_eq!(home.config.host, "100.118.61.75");
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

pub(super) fn zeroizing_secret_clone(value: &str) -> zeroize::Zeroizing<String> {
    zeroize::Zeroizing::new(value.to_string())
}

pub(super) fn zeroizing_non_empty_secret(value: &str) -> Option<zeroize::Zeroizing<String>> {
    (!value.is_empty()).then(|| zeroizing_secret_clone(value))
}
