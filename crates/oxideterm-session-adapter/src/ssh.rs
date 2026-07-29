// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionStore, SSH_CONFIG_TAG, SSH_PROXY_COMMAND_TAG, SavedAuth, SavedConnection,
    SavedConnectionRuntimeSecrets, SavedUpstreamProxyAuth, SavedUpstreamProxyPolicy, SecretString,
    resolve_ssh_config_alias,
};
use oxideterm_settings::PersistedSettings;
use oxideterm_ssh::{
    AuthMethod, ProxyCommandConfig, ProxyHopConfig, SshConfig, UpstreamProxyAuth,
    UpstreamProxyConfig, UpstreamProxyProtocol,
};

use crate::{auth_method_from_saved_auth, upstream_proxy_config_from_saved_policy};

pub fn ssh_config_from_saved_connection(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    conn: &SavedConnection,
) -> Option<SshConfig> {
    ssh_config_from_saved_connection_with_auth(store, settings, conn, None)
}

pub fn ssh_config_from_saved_connection_with_auth(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    conn: &SavedConnection,
    auth_override: Option<AuthMethod>,
) -> Option<SshConfig> {
    // An unsaved UI password can move directly into runtime auth after metadata persistence.
    let auth = auth_override.or_else(|| auth_method_from_saved_auth(store, &conn.auth))?;
    let proxy_chain = proxy_chain_config_from_saved_connection(store, conn)?;
    let proxy_command = proxy_command_from_imported_ssh_config(settings, conn);
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        // A configured proxy that cannot be hydrated must stop materialization
        // instead of silently turning into a direct connection.
        upstream_proxy: upstream_proxy_config_from_saved_policy(
            store,
            settings,
            &conn.upstream_proxy,
        )
        .ok()?,
        proxy_command,
        agent_forwarding: conn.options.agent_forwarding,
        identity_agent: conn.options.identity_agent.clone(),
        agent_forwarding_socket: conn.options.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
        strict_host_key_checking: true,
        post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        ..SshConfig::default()
    })
}

pub fn ssh_config_from_saved_connection_with_runtime_secrets(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    conn: &SavedConnection,
    mut runtime_secrets: SavedConnectionRuntimeSecrets,
    auth_override: Option<AuthMethod>,
) -> Option<SshConfig> {
    if auth_override.is_some() && runtime_secrets.auth.is_some() {
        // A target auth value must have exactly one runtime owner.
        return None;
    }
    if runtime_secrets.proxy_chain.len() != conn.proxy_chain.len() {
        return None;
    }
    let auth = match auth_override {
        Some(auth) => auth,
        None => auth_method_from_saved_auth_with_runtime_secret(
            store,
            &conn.auth,
            runtime_secrets.auth.take(),
        )?,
    };
    let proxy_chain = conn
        .proxy_chain
        .iter()
        .zip(runtime_secrets.proxy_chain)
        .map(|(hop, secret)| {
            Some(ProxyHopConfig {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth: auth_method_from_saved_auth_with_runtime_secret(store, &hop.auth, secret)?,
                agent_forwarding: hop.agent_forwarding,
                identity_agent: hop.identity_agent.clone(),
                agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let upstream_proxy = upstream_proxy_from_saved_policy_with_runtime_secret(
        store,
        settings,
        &conn.upstream_proxy,
        runtime_secrets.upstream_proxy.take(),
    )?;
    let proxy_command = proxy_command_from_imported_ssh_config(settings, conn);
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        upstream_proxy,
        proxy_command,
        agent_forwarding: conn.options.agent_forwarding,
        identity_agent: conn.options.identity_agent.clone(),
        agent_forwarding_socket: conn.options.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
        strict_host_key_checking: true,
        post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        ..SshConfig::default()
    })
}

fn auth_method_from_saved_auth_with_runtime_secret(
    store: &ConnectionStore,
    auth: &SavedAuth,
    runtime_secret: Option<SecretString>,
) -> Option<AuthMethod> {
    match (auth, runtime_secret) {
        (SavedAuth::Password { .. }, Some(password)) => {
            Some(AuthMethod::password_secret(password.into_zeroizing()))
        }
        (SavedAuth::Key { key_path, .. }, Some(passphrase)) => Some(AuthMethod::key_secret(
            key_path.clone(),
            Some(passphrase.into_zeroizing()),
        )),
        (
            SavedAuth::Certificate {
                key_path,
                cert_path,
                ..
            },
            Some(passphrase),
        ) => Some(AuthMethod::certificate_secret(
            key_path.clone(),
            cert_path.clone(),
            Some(passphrase.into_zeroizing()),
        )),
        (SavedAuth::ManagedKey { key_id, .. }, Some(passphrase)) => Some(
            AuthMethod::managed_key_secret(key_id.clone(), Some(passphrase.into_zeroizing())),
        ),
        (SavedAuth::Agent | SavedAuth::KeyboardInteractive, Some(_)) => None,
        (_, None) => auth_method_from_saved_auth(store, auth),
    }
}

fn upstream_proxy_from_saved_policy_with_runtime_secret(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    policy: &SavedUpstreamProxyPolicy,
    runtime_secret: Option<SecretString>,
) -> Option<Option<UpstreamProxyConfig>> {
    match (policy, runtime_secret) {
        (SavedUpstreamProxyPolicy::Custom { proxy }, Some(password)) => {
            let SavedUpstreamProxyAuth::Password { username, .. } = &proxy.auth else {
                return None;
            };
            Some(Some(UpstreamProxyConfig {
                protocol: match proxy.protocol {
                    oxideterm_connections::SavedUpstreamProxyProtocol::Socks5 => {
                        UpstreamProxyProtocol::Socks5
                    }
                    oxideterm_connections::SavedUpstreamProxyProtocol::HttpConnect => {
                        UpstreamProxyProtocol::HttpConnect
                    }
                },
                host: proxy.host.clone(),
                port: proxy.port,
                auth: UpstreamProxyAuth::Password {
                    username: username.clone(),
                    password: password.into_zeroizing(),
                },
                remote_dns: proxy.remote_dns,
                no_proxy: proxy.no_proxy.clone(),
            }))
        }
        (SavedUpstreamProxyPolicy::UseGlobal | SavedUpstreamProxyPolicy::Direct, Some(_)) => None,
        (_, None) => upstream_proxy_config_from_saved_policy(store, settings, policy).ok(),
    }
}

fn proxy_command_from_imported_ssh_config(
    settings: &PersistedSettings,
    connection: &SavedConnection,
) -> Option<ProxyCommandConfig> {
    if !connection.tags.iter().any(|tag| tag == SSH_CONFIG_TAG) {
        return None;
    }
    if !connection
        .tags
        .iter()
        .any(|tag| tag == SSH_PROXY_COMMAND_TAG)
    {
        return None;
    }
    let Some(host) = resolve_ssh_config_alias(&connection.name).ok().flatten() else {
        return Some(ProxyCommandConfig::Unavailable);
    };
    if host.proxy_command.is_none() {
        return Some(ProxyCommandConfig::Unavailable);
    }
    proxy_command_runtime_policy(settings.ssh_config.allow_proxy_command, host.proxy_command)
}

pub(crate) fn proxy_command_runtime_policy(
    authorized: bool,
    words: Option<Vec<oxideterm_connections::SecretString>>,
) -> Option<ProxyCommandConfig> {
    let words = words?;
    if !authorized {
        return Some(ProxyCommandConfig::AuthorizationRequired);
    }
    // ProxyCommand remains runtime-only and zeroized; it is never copied into the
    // saved connection record or exposed through diagnostics.
    ProxyCommandConfig::direct(
        words
            .into_iter()
            .map(|word| word.into_zeroizing())
            .collect(),
    )
}

pub fn proxy_chain_config_from_saved_connection(
    store: &ConnectionStore,
    conn: &SavedConnection,
) -> Option<Vec<ProxyHopConfig>> {
    conn.proxy_chain
        .iter()
        .map(|hop| {
            Some(ProxyHopConfig {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth: auth_method_from_saved_auth(store, &hop.auth)?,
                agent_forwarding: hop.agent_forwarding,
                identity_agent: hop.identity_agent.clone(),
                agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
        })
        .collect()
}

pub fn ssh_config_for_saved_connection_hop(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    connection: &SavedConnection,
    hop_index: u32,
) -> Option<SshConfig> {
    let hop_index = hop_index as usize;
    if let Some(hop) = connection.proxy_chain.get(hop_index) {
        return Some(SshConfig {
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth: auth_method_from_saved_auth(store, &hop.auth)?,
            proxy_chain: None,
            upstream_proxy: upstream_proxy_config_from_saved_policy(
                store,
                settings,
                &connection.upstream_proxy,
            )
            .ok()?,
            agent_forwarding: hop.agent_forwarding,
            identity_agent: hop.identity_agent.clone(),
            agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
            strict_host_key_checking: true,
            ..SshConfig::default()
        });
    }

    if hop_index == connection.proxy_chain.len() {
        let mut target = ssh_config_from_saved_connection(store, settings, connection)?;
        // Each node in a materialized chain connects through its parent, so the
        // per-node config must not recursively apply the persisted proxy chain.
        target.proxy_chain = None;
        return Some(target);
    }

    None
}
