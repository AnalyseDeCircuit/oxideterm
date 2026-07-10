// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{ConnectionStore, SavedConnection};
use oxideterm_settings::PersistedSettings;
use oxideterm_ssh::{ProxyHopConfig, SshConfig};

use crate::{auth_method_from_saved_auth, upstream_proxy_config_from_saved_policy};

pub fn ssh_config_from_saved_connection(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    conn: &SavedConnection,
) -> Option<SshConfig> {
    let auth = auth_method_from_saved_auth(store, &conn.auth)?;
    let proxy_chain = proxy_chain_config_from_saved_connection(store, conn)?;
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
        agent_forwarding: conn.options.agent_forwarding,
        legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
        strict_host_key_checking: true,
        post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        ..SshConfig::default()
    })
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
