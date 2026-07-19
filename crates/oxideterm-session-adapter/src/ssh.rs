// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionStore, SSH_CONFIG_TAG, SSH_PROXY_COMMAND_TAG, SavedConnection,
    resolve_ssh_config_alias,
};
use oxideterm_settings::PersistedSettings;
use oxideterm_ssh::{ProxyCommandConfig, ProxyHopConfig, SshConfig};

use crate::{auth_method_from_saved_auth, upstream_proxy_config_from_saved_policy};

pub fn ssh_config_from_saved_connection(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    conn: &SavedConnection,
) -> Option<SshConfig> {
    let auth = auth_method_from_saved_auth(store, &conn.auth)?;
    let proxy_chain = proxy_chain_config_from_saved_connection(store, conn)?;
    let proxy_command = proxy_command_from_imported_ssh_config(settings, conn);
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        proxy_chain: (!proxy_chain.is_empty()).then_some(proxy_chain),
        upstream_proxy: upstream_proxy_config_from_saved_policy(
            store,
            settings,
            &conn.upstream_proxy,
        ),
        proxy_command,
        agent_forwarding: conn.options.agent_forwarding,
        legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
        strict_host_key_checking: true,
        post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        ..SshConfig::default()
    })
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
            ),
            agent_forwarding: hop.agent_forwarding,
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
