// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionStore, SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy,
};
use oxideterm_settings::{
    PersistedSettings, SettingsUpstreamProxyAuth, SettingsUpstreamProxyConfig,
    SettingsUpstreamProxyProtocol,
};
use oxideterm_ssh::{
    UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyProtocol, upstream_proxy_from_env,
};

pub fn upstream_proxy_config_from_saved_policy(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    policy: &SavedUpstreamProxyPolicy,
) -> Result<Option<UpstreamProxyConfig>, String> {
    match policy {
        SavedUpstreamProxyPolicy::UseGlobal => match settings.network.upstream_proxy.as_ref() {
            Some(proxy) => upstream_proxy_config_from_global_settings(store, proxy).map(Some),
            None => upstream_proxy_from_env().map_err(|error| error.to_string()),
        },
        SavedUpstreamProxyPolicy::Direct => Ok(None),
        SavedUpstreamProxyPolicy::Custom { proxy } => {
            upstream_proxy_config_from_saved_proxy(store, proxy).map(Some)
        }
    }
}

pub fn upstream_proxy_config_from_global_settings(
    store: &ConnectionStore,
    proxy: &SettingsUpstreamProxyConfig,
) -> Result<UpstreamProxyConfig, String> {
    let auth = match &proxy.auth {
        SettingsUpstreamProxyAuth::None => UpstreamProxyAuth::None,
        SettingsUpstreamProxyAuth::Password {
            username,
            keychain_id,
        } => UpstreamProxyAuth::Password {
            username: username.clone(),
            // Global proxy passwords are stored separately from settings; the
            // hydrated runtime config is the only owner of this secret copy.
            password: store
                .get_global_upstream_proxy_password(
                    keychain_id
                        .as_deref()
                        .ok_or_else(|| "Global upstream proxy password is not saved".to_string())?,
                )
                .map_err(|_| "Global upstream proxy password is not available".to_string())?
                .into_zeroizing(),
        },
    };

    Ok(UpstreamProxyConfig {
        protocol: match proxy.protocol {
            SettingsUpstreamProxyProtocol::Socks5 => UpstreamProxyProtocol::Socks5,
            SettingsUpstreamProxyProtocol::HttpConnect => UpstreamProxyProtocol::HttpConnect,
        },
        host: proxy.host.clone(),
        port: proxy.port,
        auth,
        remote_dns: proxy.remote_dns,
        no_proxy: proxy.no_proxy.clone(),
    })
}

fn upstream_proxy_config_from_saved_proxy(
    store: &ConnectionStore,
    proxy: &SavedUpstreamProxyConfig,
) -> Result<UpstreamProxyConfig, String> {
    let auth = match &proxy.auth {
        SavedUpstreamProxyAuth::None => UpstreamProxyAuth::None,
        SavedUpstreamProxyAuth::Password { username, .. } => UpstreamProxyAuth::Password {
            username: username.clone(),
            // Saved connection proxy passwords follow the connection secret
            // store; hydrate them only for runtime dialing.
            password: store
                .get_saved_upstream_proxy_password(&proxy.auth)
                .map_err(|_| "Connection upstream proxy password is not available".to_string())?
                .into_zeroizing(),
        },
    };

    Ok(UpstreamProxyConfig {
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
        auth,
        remote_dns: proxy.remote_dns,
        no_proxy: proxy.no_proxy.clone(),
    })
}
