// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionStore, ConnectionX11ForwardingMode, ConnectionX11ForwardingOptions,
    SavedProxyCommand, SavedUpstreamProxyProtocol, SecretString, first_available_default_key_path,
};
use oxideterm_remote_desktop::RemoteDesktopProtocol;
use oxideterm_session_adapter::{auth_method_from_saved_auth, proxy_command_from_value};
use oxideterm_ssh::{
    AuthMethod, NativeSessionTreeConnectEndpoint, NodeId, NodeRouter, ProxyHopConfig, SshConfig,
    UpstreamProxyAuth, UpstreamProxyProtocol, X11ForwardPolicy, X11ForwardTrust,
};

pub fn runtime_proxy_hops_for_parent_path(
    router: &NodeRouter,
    parent: &NodeId,
    i18n: &oxideterm_i18n::I18n,
) -> anyhow::Result<Vec<NewConnectionProxyHop>> {
    let mut configs = Vec::new();
    let mut next = Some(parent.clone());
    while let Some(node) = next {
        let snapshot = router.node_runtime_snapshot(&node).ok_or_else(|| {
            anyhow::anyhow!("{}: {}", i18n.t("ssh.form.runtime_node_missing"), node.0)
        })?;
        configs.push(snapshot.config);
        next = snapshot.parent_id;
    }
    Ok(configs
        .into_iter()
        .rev()
        .flat_map(|config| {
            config
                .proxy_chain
                .unwrap_or_default()
                .into_iter()
                .chain(std::iter::once(ProxyHopConfig {
                    totp: config.totp,
                    host: config.host,
                    port: config.port,
                    username: config.username,
                    auth: config.auth,
                    agent_forwarding: config.agent_forwarding,
                    identity_agent: config.identity_agent,
                    agent_forwarding_socket: config.agent_forwarding_socket,
                    legacy_ssh_compatibility: config.legacy_ssh_compatibility,
                    ssh_algorithms: config.ssh_algorithms,
                    strict_host_key_checking: true,
                    trust_host_key: None,
                    expected_host_key_fingerprint: None,
                }))
                .map(proxy_hop_form_from_runtime_config)
        })
        .collect())
}

pub fn form_for_runtime_node(
    router: &NodeRouter,
    node: &NodeId,
    title: &str,
    save: bool,
    i18n: &oxideterm_i18n::I18n,
) -> anyhow::Result<NewConnectionForm> {
    let snapshot = router
        .node_runtime_snapshot(node)
        .ok_or_else(|| anyhow::anyhow!(i18n.t("ssh.form.runtime_node_missing")))?;
    let mut form =
        form_from_runtime_config(snapshot.config, Some(title), i18n.t("ssh.form.ungrouped"));
    if save && let Some(parent) = snapshot.parent_id.as_ref() {
        form.proxy_hops = runtime_proxy_hops_for_parent_path(router, parent, i18n)?;
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    }
    form.agent_available = detect_ssh_agent_available(&form.identity_agent);
    form.save_connection = save;
    Ok(form)
}
use zeroize::Zeroize;

use crate::{
    configuration::{RuntimeSecretHandoff, upstream_proxy_config_from_form},
    form_state::{
        NewConnectionForm, NewConnectionProxyHop, NewConnectionTransport,
        NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy, SshAuthTab,
        identity_agent_from_form, identity_agent_selector,
    },
};

fn connection_x11_options(policy: Option<X11ForwardPolicy>) -> ConnectionX11ForwardingOptions {
    let Some(policy) = policy else {
        return ConnectionX11ForwardingOptions::default();
    };
    let mode = match policy.trust {
        X11ForwardTrust::Trusted => ConnectionX11ForwardingMode::Trusted,
        X11ForwardTrust::Untrusted => ConnectionX11ForwardingMode::Untrusted,
    };
    let default_timeout = ConnectionX11ForwardingOptions::default().untrusted_timeout_seconds;
    let untrusted_timeout_seconds = match (policy.trust, policy.timeout_millis) {
        (X11ForwardTrust::Untrusted, None) => 0,
        (_, Some(value)) => u32::try_from(value / 1_000)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(default_timeout),
        (X11ForwardTrust::Trusted, None) => default_timeout,
    };
    ConnectionX11ForwardingOptions {
        enabled: true,
        mode,
        untrusted_timeout_seconds,
    }
}

pub fn x11_forward_policy(options: ConnectionX11ForwardingOptions) -> Option<X11ForwardPolicy> {
    if !options.enabled {
        return None;
    }
    match options.mode {
        ConnectionX11ForwardingMode::Trusted => Some(X11ForwardPolicy::trusted()),
        ConnectionX11ForwardingMode::Untrusted if options.untrusted_timeout_seconds == 0 => {
            Some(X11ForwardPolicy::untrusted().without_timeout())
        }
        ConnectionX11ForwardingMode::Untrusted => Some(
            X11ForwardPolicy::untrusted()
                .with_timeout_millis(u64::from(options.untrusted_timeout_seconds) * 1_000),
        ),
    }
}

pub fn build_ssh_config(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    settings: &oxideterm_settings::PersistedSettings,
    i18n: &oxideterm_i18n::I18n,
    secret_handoff: RuntimeSecretHandoff,
) -> Result<(SshConfig, String), String> {
    let host = form.host.trim().to_string();
    let username = form.username.trim().to_string();
    let port = form.port.trim().parse::<u16>().ok();
    if host.is_empty() || username.is_empty() || port.is_none() {
        return Err(i18n.t("ssh.form.validation_required"));
    }
    match form.auth_tab {
        SshAuthTab::Password
        | SshAuthTab::Agent
        | SshAuthTab::DefaultKey
        | SshAuthTab::TwoFactor => {}
        SshAuthTab::SshKey => {
            if form.key_path.trim().is_empty() {
                return Err(i18n.t("ssh.form.key_path_required"));
            }
        }
        SshAuthTab::ManagedKey => {
            if form.managed_key_id.trim().is_empty() {
                return Err(i18n.t("ssh.form.managed_key_required"));
            }
        }
        SshAuthTab::Certificate => {
            if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() {
                return Err(i18n.t("ssh.form.certificate_paths_required"));
            }
        }
    }
    let saved_proxy_hop_auth = if form.proxy_command_enabled {
        Vec::new()
    } else {
        if let Err(error) = validate_proxy_chain_form(form) {
            return Err(error);
        }
        let missing_credentials_message = i18n.t("sessions.saved_next_hop.missing_credentials");
        match saved_proxy_hop_auth_from_store(store, form, &missing_credentials_message) {
            Ok(saved_auth) => saved_auth,
            Err(error) => {
                return Err(error);
            }
        }
    };
    // Resolve every fallible route input before moving authentication drafts.
    let proxy_command = if form.proxy_command_enabled {
        let command = if form.proxy_command.trim().is_empty() {
            let saved_command = SavedProxyCommand {
                keychain_id: form.proxy_command_keychain_id.clone(),
                plaintext_command: None,
            };
            match store.get_saved_proxy_command(&saved_command) {
                Ok(command) => command,
                Err(error) => {
                    return Err(error.to_string());
                }
            }
        } else {
            SecretString::from(secret_handoff.zeroizing(&mut form.proxy_command))
        };
        let proxy_alias = if form.name.trim().is_empty() {
            host.as_str()
        } else {
            form.name.trim()
        };
        Some(proxy_command_from_value(
            settings.ssh_config.allow_proxy_command,
            command,
            proxy_alias,
            &host,
            Some(&username),
            port,
        ))
    } else {
        None
    };
    let upstream_proxy = if proxy_command.is_some() {
        None
    } else {
        match upstream_proxy_config_from_form(store, settings, form, secret_handoff) {
            Ok(upstream_proxy) => upstream_proxy,
            Err(error) => {
                return Err(error.to_string());
            }
        }
    };
    let fallback_auth = match form.auth_tab {
        SshAuthTab::Password => {
            if form.empty_password {
                form.password.zeroize();
                AuthMethod::password("")
            } else if form.password.is_empty() {
                AuthMethod::password_prompt()
            } else {
                AuthMethod::password_secret(secret_handoff.zeroizing(&mut form.password))
            }
        }
        SshAuthTab::Agent => AuthMethod::Agent,
        SshAuthTab::DefaultKey => {
            AuthMethod::key_secret("", secret_handoff.zeroizing_non_empty(&mut form.passphrase))
        }
        SshAuthTab::SshKey => AuthMethod::key_secret(
            form.key_path.trim().to_string(),
            secret_handoff.zeroizing_non_empty(&mut form.passphrase),
        ),
        SshAuthTab::ManagedKey => {
            // Runtime auth carries only the managed-key reference and runtime-owned passphrase.
            AuthMethod::managed_key_secret(
                form.managed_key_id.trim().to_string(),
                secret_handoff.zeroizing_non_empty(&mut form.passphrase),
            )
        }
        SshAuthTab::Certificate => AuthMethod::certificate_secret(
            form.key_path.trim().to_string(),
            form.cert_path.trim().to_string(),
            secret_handoff.zeroizing_non_empty(&mut form.passphrase),
        ),
        SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
    };
    let auth = if form.gssapi_enabled {
        AuthMethod::kerberos_preferred(
            fallback_auth,
            (!form.gssapi_server_identity.trim().is_empty())
                .then(|| form.gssapi_server_identity.trim().to_string()),
            form.gssapi_delegate_credentials,
        )
    } else {
        fallback_auth
    };
    let proxy_chain = if proxy_command.is_some() {
        None
    } else {
        proxy_chain_from_form(store, form, secret_handoff, saved_proxy_hop_auth)
    };
    let config = SshConfig {
        totp: store.totp_binding(form.totp_credential_id.as_deref()),
        host: host.clone(),
        port: port.unwrap_or(22),
        username: username.clone(),
        auth,
        timeout_secs: form.connect_timeout_seconds,
        keep_alive_interval_secs: form.keep_alive_interval_seconds,
        keep_alive_count_max: form.keep_alive_count_max,
        agent_forwarding: !form.ssh_channel_strategy.requires_dedicated_consumers()
            && form.agent_forwarding,
        identity_agent: identity_agent_from_form(&form.identity_agent),
        agent_forwarding_socket: form.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: form.legacy_ssh_compatibility,
        ssh_channel_strategy: form.ssh_channel_strategy,
        ssh_algorithms: form.ssh_algorithms.clone(),
        x11_forwarding: (!form.ssh_channel_strategy.requires_dedicated_consumers())
            .then(|| x11_forward_policy(form.x11_forwarding))
            .flatten(),
        proxy_chain,
        upstream_proxy,
        proxy_command,
        strict_host_key_checking: true,
        post_connect_command: (!form.post_connect_command.trim().is_empty())
            .then(|| form.post_connect_command.trim().to_string()),
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        ..SshConfig::default()
    };
    let title = if form.name.trim().is_empty() {
        format!("{username}@{host}")
    } else {
        form.name.trim().to_string()
    };
    Ok((config, title))
}

pub fn detect_ssh_agent_available(identity_agent: &str) -> Option<bool> {
    oxideterm_ssh::ssh_agent_available(identity_agent_selector(identity_agent))
}

pub fn proxy_chain_from_form(
    store: &ConnectionStore,
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
            totp: store.totp_binding(hop.totp_credential_id.as_deref()),
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

pub fn saved_proxy_hop_auth_from_store(
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

pub fn validate_proxy_chain_form(form: &NewConnectionForm) -> Result<(), String> {
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

pub fn proxy_session_tree_endpoints(config: &SshConfig) -> Vec<NativeSessionTreeConnectEndpoint> {
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

pub fn prepare_proxy_chain_test_config(config: &mut SshConfig) {
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

pub fn prepare_tree_connect_config(config: &mut SshConfig) -> Result<(), String> {
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

pub fn resolve_default_key_for_tree_auth(auth: &mut AuthMethod) -> Result<(), String> {
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

fn auth_method_from_proxy_hop(
    hop: &mut NewConnectionProxyHop,
    secret_handoff: RuntimeSecretHandoff,
) -> AuthMethod {
    let fallback = match hop.auth_tab {
        SshAuthTab::Password => {
            if hop.empty_password {
                hop.password.zeroize();
                AuthMethod::password("")
            } else if hop.password.is_empty() {
                AuthMethod::password_prompt()
            } else {
                AuthMethod::password_secret(secret_handoff.zeroizing(&mut hop.password))
            }
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

pub fn form_from_runtime_config(
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
    form.empty_password = auth_fields.empty_password;
    form.key_path = auth_fields.key_path;
    form.managed_key_id = auth_fields.managed_key_id;
    form.cert_path = auth_fields.cert_path;
    form.passphrase = auth_fields.passphrase;
    form.gssapi_enabled = auth_fields.gssapi_enabled;
    form.gssapi_server_identity = auth_fields.gssapi_server_identity;
    form.gssapi_delegate_credentials = auth_fields.gssapi_delegate_credentials;
    form.group = default_group;
    form.post_connect_command = config.post_connect_command.clone().unwrap_or_default();
    form.pre_connect_command = config.pre_connect_command.clone().unwrap_or_default();
    form.agent_forwarding = config.agent_forwarding;
    form.identity_agent = config.identity_agent.clone().unwrap_or_default();
    form.agent_forwarding_socket = config.agent_forwarding_socket.clone();
    form.legacy_ssh_compatibility = config.legacy_ssh_compatibility;
    form.ssh_channel_strategy = config.ssh_channel_strategy;
    form.connect_timeout_seconds = config.timeout_secs;
    form.connect_timeout_seconds_text = config.timeout_secs.to_string();
    form.keep_alive_interval_seconds = config.keep_alive_interval_secs;
    form.keep_alive_interval_seconds_text = config.keep_alive_interval_secs.to_string();
    form.keep_alive_count_max = config.keep_alive_count_max;
    form.keep_alive_count_max_text = config.keep_alive_count_max.to_string();
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

pub fn proxy_hop_form_from_runtime_config(config: ProxyHopConfig) -> NewConnectionProxyHop {
    let auth_fields = runtime_auth_form_fields(config.auth);
    NewConnectionProxyHop {
        totp_credential_id: config.totp.map(|binding| binding.credential_id),
        totp_picker_expanded: false,
        empty_password: auth_fields.empty_password,
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
    empty_password: bool,
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
        AuthMethod::Password {
            mut password,
            prompt,
        } => RuntimeAuthFormFields {
            empty_password: !prompt && password.is_empty(),
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
            empty_password: false,
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
            empty_password: false,
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
            empty_password: false,
            auth_tab: SshAuthTab::ManagedKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: key_id,
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
            empty_password: false,
            auth_tab: SshAuthTab::Certificate,
            password: String::new(),
            key_path: key_path,
            managed_key_id: String::new(),
            cert_path: cert_path,
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
            empty_password: false,
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
            empty_password: false,
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
    fn proxy_auth_handoff_preserves_test_drafts_and_moves_connect_drafts() {
        for (handoff, expected_draft) in [
            (RuntimeSecretHandoff::CopyForTest, "jump-secret"),
            (RuntimeSecretHandoff::Move, ""),
        ] {
            let mut hop = NewConnectionProxyHop::new();
            hop.auth_tab = SshAuthTab::Password;
            hop.password = "jump-secret".to_string();
            let allocation = hop.password.as_ptr();
            let auth = auth_method_from_proxy_hop(&mut hop, handoff);
            let AuthMethod::Password { password, prompt } = auth else {
                panic!("expected password authentication");
            };
            assert_eq!(password.as_str(), "jump-secret", "{handoff:?}");
            assert!(!prompt, "{handoff:?}");
            assert_eq!(hop.password, expected_draft, "{handoff:?}");
            if matches!(handoff, RuntimeSecretHandoff::Move) {
                assert_eq!(password.as_ptr(), allocation);
            }
        }
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
        let proxy_chain = proxy_chain_from_form(
            &connection_store,
            &mut form,
            RuntimeSecretHandoff::CopyForTest,
            saved_auth,
        )
        .expect("proxy chain");

        assert!(matches!(
            &proxy_chain[0].auth,
            AuthMethod::Password { password, .. } if password.as_str() == "public-proxy-secret"
        ));
        assert!(matches!(
            &proxy_chain[1].auth,
            AuthMethod::Password { password, .. } if password.as_str() == "gateway-secret"
        ));
        assert!(form.proxy_hops.iter().all(|hop| hop.password.is_empty()));
        let debug_output = format!("{proxy_chain:?}");
        assert!(!debug_output.contains("public-proxy-secret"));
        assert!(!debug_output.contains("gateway-secret"));
    }

    #[test]
    fn runtime_proxy_hop_form_preserves_password_for_save_as() {
        let hop = proxy_hop_form_from_runtime_config(ProxyHopConfig {
            totp: None,
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
            totp: None,
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
}

pub fn serial_profile_name_or_port(name: &str, port_path: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        port_path.to_string()
    } else {
        name.to_string()
    }
}

pub fn telnet_profile_name_or_endpoint(name: &str, host: &str, port: u16) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!("{}:{}", host.trim(), port)
    } else {
        name.to_string()
    }
}

pub fn remote_desktop_protocol_for_transport(
    transport: NewConnectionTransport,
) -> Option<RemoteDesktopProtocol> {
    match transport {
        NewConnectionTransport::Rdp => Some(RemoteDesktopProtocol::Rdp),
        NewConnectionTransport::Vnc => Some(RemoteDesktopProtocol::Vnc),
        _ => None,
    }
}

pub fn remote_desktop_profile_label(
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

fn remote_desktop_label_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        // Keep IPv6 endpoint labels parseable when shown in tab titles.
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

pub fn serial_profile_group_from_form(group: &str, i18n: &oxideterm_i18n::I18n) -> Option<String> {
    (!connection_group_is_ungrouped(group, i18n)).then(|| group.trim().to_owned())
}

pub use crate::form_state::connection_group_is_ungrouped;

pub fn asset_icon_from_form(icon: &str) -> Option<String> {
    // Empty means the asset should use its transport-specific fallback icon.
    let icon = icon.trim();
    (!icon.is_empty()).then(|| icon.to_string())
}

pub fn asset_color_from_form(color: &str) -> Option<String> {
    // Empty means the asset should use its transport-specific fallback color.
    let color = color.trim();
    (!color.is_empty()).then(|| color.to_string())
}

pub fn serial_profile_parity_from_terminal(
    parity: oxideterm_terminal::SerialParity,
) -> oxideterm_connections::SerialParity {
    match parity {
        oxideterm_terminal::SerialParity::None => oxideterm_connections::SerialParity::None,
        oxideterm_terminal::SerialParity::Odd => oxideterm_connections::SerialParity::Odd,
        oxideterm_terminal::SerialParity::Even => oxideterm_connections::SerialParity::Even,
    }
}

pub fn serial_profile_flow_from_terminal(
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

pub fn take_zeroizing_secret(value: &mut String) -> zeroize::Zeroizing<String> {
    // Preserve the UI allocation while transferring it to the runtime secret owner.
    zeroize::Zeroizing::new(std::mem::take(value))
}

mod serial;
pub use serial::*;
