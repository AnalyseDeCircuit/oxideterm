// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionStore, ConnectionX11ForwardingMode, ConnectionX11ForwardingOptions, SSH_CONFIG_TAG,
    SSH_PROXY_COMMAND_TAG, SavedAuth, SavedConnection, SavedConnectionRuntimeSecrets,
    SavedStandaloneSftpEndpointRuntimeSecrets, SavedStandaloneSftpProfileRuntimeSecrets,
    SavedUpstreamProxyAuth, SavedUpstreamProxyPolicy, SecretString, StandaloneSftpEndpoint,
    StandaloneSftpProfile, resolve_proxy_command, resolve_ssh_config_alias,
};
use oxideterm_settings::PersistedSettings;
use oxideterm_ssh::{
    AuthMethod, ProxyCommandConfig, ProxyHopConfig, SshConfig, UpstreamProxyAuth,
    UpstreamProxyConfig, UpstreamProxyProtocol, X11ForwardPolicy,
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
    let proxy_command = proxy_command_from_saved_connection(store, settings, conn, None);
    let proxy_chain = if proxy_command.is_some() {
        Vec::new()
    } else if conn.proxy_chain.is_empty() {
        legacy_jump_host_proxy_chain(store, conn)?
    } else {
        proxy_chain_config_from_saved_connection(store, conn)?
    };
    let upstream_proxy = if proxy_command.is_some() {
        None
    } else {
        // A configured proxy that cannot be hydrated must stop materialization
        // instead of silently turning into a direct connection.
        upstream_proxy_config_from_saved_policy(store, settings, &conn.upstream_proxy).ok()?
    };
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        timeout_secs: conn.options.effective_connect_timeout_seconds(),
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        upstream_proxy,
        proxy_command,
        agent_forwarding: conn.options.agent_forwarding,
        identity_agent: conn.options.identity_agent.clone(),
        agent_forwarding_socket: conn.options.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
        ssh_algorithms: conn.options.ssh_algorithms.clone(),
        x11_forwarding: x11_forward_policy(conn.options.x11_forwarding),
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
    let proxy_command = proxy_command_from_saved_connection(
        store,
        settings,
        conn,
        runtime_secrets.proxy_command.take(),
    );
    let proxy_chain = if proxy_command.is_some() {
        Vec::new()
    } else if conn.proxy_chain.is_empty() {
        legacy_jump_host_proxy_chain(store, conn)?
    } else {
        conn.proxy_chain
            .iter()
            .zip(runtime_secrets.proxy_chain)
            .map(|(hop, secret)| {
                Some(ProxyHopConfig {
                    host: hop.host.clone(),
                    port: hop.port,
                    username: hop.username.clone(),
                    auth: auth_method_from_saved_auth_with_runtime_secret(
                        store, &hop.auth, secret,
                    )?,
                    agent_forwarding: hop.agent_forwarding,
                    identity_agent: hop.identity_agent.clone(),
                    agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                    legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                    ssh_algorithms: hop.ssh_algorithms.clone(),
                    strict_host_key_checking: true,
                    trust_host_key: None,
                    expected_host_key_fingerprint: None,
                })
            })
            .collect::<Option<Vec<_>>>()?
    };
    let upstream_proxy = if proxy_command.is_some() {
        None
    } else {
        upstream_proxy_from_saved_policy_with_runtime_secret(
            store,
            settings,
            &conn.upstream_proxy,
            runtime_secrets.upstream_proxy.take(),
        )?
    };
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        timeout_secs: conn.options.effective_connect_timeout_seconds(),
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        upstream_proxy,
        proxy_command,
        agent_forwarding: conn.options.agent_forwarding,
        identity_agent: conn.options.identity_agent.clone(),
        agent_forwarding_socket: conn.options.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
        ssh_algorithms: conn.options.ssh_algorithms.clone(),
        x11_forwarding: x11_forward_policy(conn.options.x11_forwarding),
        strict_host_key_checking: true,
        post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        ..SshConfig::default()
    })
}

/// Materializes one independent SFTP endpoint without assigning NodeRouter ownership.
pub fn ssh_config_from_standalone_sftp_profile_with_runtime_secrets(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    profile: &StandaloneSftpProfile,
    mut runtime_secrets: SavedStandaloneSftpProfileRuntimeSecrets,
    auth_override: Option<AuthMethod>,
) -> Option<SshConfig> {
    let primary = StandaloneSftpEndpoint {
        host: profile.host.clone(),
        port: profile.port,
        username: profile.username.clone(),
        auth: profile.auth.clone(),
        connect_timeout_seconds: profile.connect_timeout_seconds,
        proxy_chain: profile.proxy_chain.clone(),
        upstream_proxy: profile.upstream_proxy.clone(),
        proxy_command: profile.proxy_command.clone(),
        identity_agent: profile.identity_agent.clone(),
        legacy_ssh_compatibility: profile.legacy_ssh_compatibility,
        ssh_algorithms: profile.ssh_algorithms.clone(),
        initial_remote_path: profile.initial_remote_path.clone(),
    };
    let primary_secrets = SavedStandaloneSftpEndpointRuntimeSecrets {
        auth: runtime_secrets.auth.take(),
        proxy_chain: std::mem::take(&mut runtime_secrets.proxy_chain),
        upstream_proxy: runtime_secrets.upstream_proxy.take(),
        proxy_command: runtime_secrets.proxy_command.take(),
    };
    ssh_config_from_standalone_sftp_endpoint_with_runtime_secrets(
        store,
        settings,
        &profile.name,
        &primary,
        primary_secrets,
        auth_override,
    )
}

/// Materializes one endpoint of a standalone SFTP profile without NodeRouter ownership.
pub fn ssh_config_from_standalone_sftp_endpoint_with_runtime_secrets(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    profile_name: &str,
    endpoint: &StandaloneSftpEndpoint,
    mut runtime_secrets: SavedStandaloneSftpEndpointRuntimeSecrets,
    auth_override: Option<AuthMethod>,
) -> Option<SshConfig> {
    if auth_override.is_some() && runtime_secrets.auth.is_some() {
        return None;
    }
    if runtime_secrets.proxy_chain.len() != endpoint.proxy_chain.len() {
        return None;
    }
    let auth = match auth_override {
        Some(auth) => auth,
        None => auth_method_from_saved_auth_with_runtime_secret(
            store,
            &endpoint.auth,
            runtime_secrets.auth.take(),
        )?,
    };
    let proxy_command = endpoint.proxy_command.as_ref().map(|saved_command| {
        if !settings.ssh_config.allow_proxy_command {
            return ProxyCommandConfig::AuthorizationRequired;
        }
        let command = runtime_secrets
            .proxy_command
            .take()
            .or_else(|| store.get_saved_proxy_command(saved_command).ok());
        command.map_or(ProxyCommandConfig::Unavailable, |command| {
            proxy_command_from_value(
                true,
                command,
                profile_name,
                &endpoint.host,
                Some(&endpoint.username),
                Some(endpoint.port),
            )
        })
    });
    let proxy_chain = if proxy_command.is_some() {
        Vec::new()
    } else {
        endpoint
            .proxy_chain
            .iter()
            .zip(runtime_secrets.proxy_chain)
            .map(|(hop, secret)| {
                Some(ProxyHopConfig {
                    host: hop.host.clone(),
                    port: hop.port,
                    username: hop.username.clone(),
                    auth: auth_method_from_saved_auth_with_runtime_secret(
                        store, &hop.auth, secret,
                    )?,
                    agent_forwarding: hop.agent_forwarding,
                    identity_agent: hop.identity_agent.clone(),
                    agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
                    legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
                    ssh_algorithms: hop.ssh_algorithms.clone(),
                    strict_host_key_checking: true,
                    trust_host_key: None,
                    expected_host_key_fingerprint: None,
                })
            })
            .collect::<Option<Vec<_>>>()?
    };
    let upstream_proxy = if proxy_command.is_some() {
        None
    } else {
        upstream_proxy_from_saved_policy_with_runtime_secret(
            store,
            settings,
            &endpoint.upstream_proxy,
            runtime_secrets.upstream_proxy.take(),
        )?
    };
    Some(SshConfig {
        host: endpoint.host.clone(),
        port: endpoint.port,
        username: endpoint.username.clone(),
        auth,
        timeout_secs: endpoint.connect_timeout_seconds,
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        upstream_proxy,
        proxy_command,
        identity_agent: endpoint.identity_agent.clone(),
        legacy_ssh_compatibility: endpoint.legacy_ssh_compatibility,
        ssh_algorithms: endpoint.ssh_algorithms.clone(),
        strict_host_key_checking: true,
        ..SshConfig::default()
    })
}

fn x11_forward_policy(options: ConnectionX11ForwardingOptions) -> Option<X11ForwardPolicy> {
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

fn auth_method_from_saved_auth_with_runtime_secret(
    store: &ConnectionStore,
    auth: &SavedAuth,
    runtime_secret: Option<SecretString>,
) -> Option<AuthMethod> {
    match (auth, runtime_secret) {
        (
            SavedAuth::KerberosPreferred {
                server_identity,
                delegate_credentials,
                fallback,
            },
            runtime_secret,
        ) => Some(AuthMethod::kerberos_preferred(
            auth_method_from_saved_auth_with_runtime_secret(store, fallback, runtime_secret)?,
            server_identity.clone(),
            *delegate_credentials,
        )),
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

fn proxy_command_from_saved_connection(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    connection: &SavedConnection,
    runtime_command: Option<SecretString>,
) -> Option<ProxyCommandConfig> {
    if let Some(saved_command) = connection.proxy_command.as_ref() {
        if !settings.ssh_config.allow_proxy_command {
            return Some(ProxyCommandConfig::AuthorizationRequired);
        }
        let command = runtime_command.or_else(|| store.get_saved_proxy_command(saved_command).ok());
        return Some(command.map_or(ProxyCommandConfig::Unavailable, |command| {
            proxy_command_from_value(
                true,
                command,
                &connection.name,
                &connection.host,
                Some(&connection.username),
                Some(connection.port),
            )
        }));
    }
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

pub fn proxy_command_from_value(
    authorized: bool,
    command: SecretString,
    alias: &str,
    hostname: &str,
    user: Option<&str>,
    port: Option<u16>,
) -> ProxyCommandConfig {
    if !authorized {
        return ProxyCommandConfig::AuthorizationRequired;
    }
    proxy_command_runtime_policy(
        true,
        Some(resolve_proxy_command(command, alias, hostname, user, port)),
    )
    .unwrap_or(ProxyCommandConfig::Unavailable)
}

pub fn proxy_chain_config_from_saved_connection(
    store: &ConnectionStore,
    conn: &SavedConnection,
) -> Option<Vec<ProxyHopConfig>> {
    if conn.proxy_chain.is_empty() {
        return legacy_jump_host_proxy_chain(store, conn);
    }
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
                ssh_algorithms: hop.ssh_algorithms.clone(),
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
        })
        .collect()
}

fn legacy_jump_host_proxy_chain(
    store: &ConnectionStore,
    connection: &SavedConnection,
) -> Option<Vec<ProxyHopConfig>> {
    let Some(jump_id) = connection.options.jump_host.as_deref() else {
        return Some(Vec::new());
    };
    if jump_id == connection.id {
        return None;
    }
    let jump = store.get(jump_id)?;
    Some(vec![ProxyHopConfig {
        host: jump.host.clone(),
        port: jump.port,
        username: jump.username.clone(),
        auth: auth_method_from_saved_auth(store, &jump.auth)?,
        agent_forwarding: jump.options.agent_forwarding,
        identity_agent: jump.options.identity_agent.clone(),
        agent_forwarding_socket: jump.options.agent_forwarding_socket.clone(),
        legacy_ssh_compatibility: jump.options.legacy_ssh_compatibility,
        ssh_algorithms: jump.options.ssh_algorithms.clone(),
        strict_host_key_checking: true,
        trust_host_key: None,
        expected_host_key_fingerprint: None,
    }])
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
            timeout_secs: connection.options.effective_connect_timeout_seconds(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_x11_options_become_non_secret_runtime_policy() {
        let policy = x11_forward_policy(ConnectionX11ForwardingOptions {
            enabled: true,
            mode: ConnectionX11ForwardingMode::Untrusted,
            untrusted_timeout_seconds: 900,
        })
        .unwrap();

        assert_eq!(policy.timeout_millis, Some(900_000));
        assert!(!policy.is_trusted());
        assert_eq!(
            x11_forward_policy(ConnectionX11ForwardingOptions {
                enabled: true,
                mode: ConnectionX11ForwardingMode::Untrusted,
                untrusted_timeout_seconds: 0,
            })
            .unwrap()
            .timeout_millis,
            None
        );
        assert!(x11_forward_policy(ConnectionX11ForwardingOptions::default()).is_none());
    }
}
