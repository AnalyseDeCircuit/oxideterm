use crate::{SyncScope, operation::StructuredUploadItemFilter};
use anyhow::Result;
use oxideterm_connections::{
    ConnectionStore, CredentialSyncSelection, ProfileCredentialRestoreSummary,
    SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyProtocol,
    oxide_file::EncryptedPortableSecret,
};
use oxideterm_settings::{
    PersistedSettings, SettingsUpstreamProxyAuth, SettingsUpstreamProxyProtocol,
};

pub(crate) struct ProfileCredentialImport {
    pub secrets: Vec<EncryptedPortableSecret>,
    pub selection: CredentialSyncSelection,
    pub summary: ProfileCredentialRestoreSummary,
}

pub(crate) fn credential_selection(
    store: &ConnectionStore,
    scope: &SyncScope,
    filter: &StructuredUploadItemFilter,
) -> CredentialSyncSelection {
    let mut result = CredentialSyncSelection::default();
    if scope.sync_connections {
        result.connection_ids = store
            .connections()
            .iter()
            .filter(|p| {
                filter
                    .connection_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&p.id))
            })
            .map(|p| p.id.clone())
            .collect();
        result.sftp_ids = store
            .standalone_sftp_profiles()
            .iter()
            .map(|p| p.id.clone())
            .collect();
    }
    if scope.sync_mosh_profiles {
        result.mosh_ids = store
            .mosh_profiles()
            .iter()
            .filter(|p| {
                filter
                    .mosh_profile_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&p.id))
            })
            .map(|p| p.id.clone())
            .collect();
    }
    if scope.sync_remote_desktop_profiles {
        result.remote_desktop_ids = store
            .remote_desktop_profiles()
            .iter()
            .filter(|p| {
                filter
                    .remote_desktop_profile_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&p.id))
            })
            .map(|p| p.id.clone())
            .collect();
    }
    result.global_proxy = scope.sync_app_settings
        && scope
            .app_settings_sections
            .iter()
            .any(|section| section == "network");
    result
}

pub(crate) fn global_proxy(settings: &PersistedSettings) -> Option<SavedUpstreamProxyConfig> {
    settings
        .network
        .upstream_proxy
        .as_ref()
        .map(|proxy| SavedUpstreamProxyConfig {
            protocol: match proxy.protocol {
                SettingsUpstreamProxyProtocol::Socks5 => SavedUpstreamProxyProtocol::Socks5,
                SettingsUpstreamProxyProtocol::HttpConnect => {
                    SavedUpstreamProxyProtocol::HttpConnect
                }
            },
            host: proxy.host.clone(),
            port: proxy.port,
            remote_dns: proxy.remote_dns,
            no_proxy: proxy.no_proxy.clone(),
            auth: match &proxy.auth {
                SettingsUpstreamProxyAuth::None => SavedUpstreamProxyAuth::None,
                SettingsUpstreamProxyAuth::Password {
                    username,
                    keychain_id,
                } => SavedUpstreamProxyAuth::Password {
                    username: username.clone(),
                    keychain_id: keychain_id.clone(),
                    plaintext_password: None,
                },
            },
        })
}

pub(crate) fn apply_global_proxy_reference(
    settings: &mut PersistedSettings,
    proxy: Option<&SavedUpstreamProxyConfig>,
) {
    if let Some(proxy) = proxy
        && let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &proxy.auth
        && let Some(settings_proxy) = settings.network.upstream_proxy.as_mut()
        && let SettingsUpstreamProxyAuth::Password {
            keychain_id: target,
            ..
        } = &mut settings_proxy.auth
    {
        *target = keychain_id.clone();
    }
}

pub(crate) fn preserve_global_proxy_reference(
    current: &PersistedSettings,
    next: &mut PersistedSettings,
) {
    let before = global_proxy(current);
    let after = global_proxy(next);
    if let Some(mut incoming) = after {
        if let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &mut incoming.auth {
            *keychain_id = None;
        }
        if let Some(mut previous) = before {
            let old_auth = previous.auth.clone();
            if let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &mut previous.auth {
                *keychain_id = None;
            }
            if previous.protocol == incoming.protocol
                && previous.host == incoming.host
                && previous.port == incoming.port
                && matches!((&previous.auth, &incoming.auth), (SavedUpstreamProxyAuth::Password { username: a, .. }, SavedUpstreamProxyAuth::Password { username: b, .. }) if a == b)
            {
                incoming.auth = old_auth;
            }
        }
        apply_global_proxy_reference(next, Some(&incoming));
    }
}

pub(crate) fn export_profile_credentials(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    scope: &SyncScope,
    filter: &StructuredUploadItemFilter,
) -> Result<Vec<EncryptedPortableSecret>> {
    store.export_profile_credentials(
        &credential_selection(store, scope, filter),
        global_proxy(settings).as_ref(),
    )
}

/// Counts selected portable credentials without opening protected storage.
pub fn profile_credential_count_for_scope(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    scope: &SyncScope,
    filter: &StructuredUploadItemFilter,
) -> usize {
    if !scope.sync_sensitive_credentials {
        return 0;
    }
    store.profile_credential_count(
        &credential_selection(store, scope, filter),
        global_proxy(settings).as_ref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_settings::SettingsUpstreamProxyConfig;

    fn settings(reference: &str) -> PersistedSettings {
        let mut settings = PersistedSettings::default();
        settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
            protocol: SettingsUpstreamProxyProtocol::Socks5,
            host: "proxy.test".into(),
            port: 1080,
            auth: SettingsUpstreamProxyAuth::Password {
                username: "user".into(),
                keychain_id: Some(reference.into()),
            },
            remote_dns: true,
            no_proxy: String::new(),
        });
        settings
    }

    #[test]
    fn metadata_only_proxy_sync_keeps_local_reference_only_for_matching_endpoint_and_account() {
        let local = settings("local-reference");
        let mut incoming = settings("foreign-reference");
        preserve_global_proxy_reference(&local, &mut incoming);
        let Some(proxy) = global_proxy(&incoming) else {
            panic!("proxy config");
        };
        assert!(
            matches!(proxy.auth, SavedUpstreamProxyAuth::Password { keychain_id: Some(ref key), .. } if key == "local-reference")
        );
        incoming.network.upstream_proxy.as_mut().unwrap().host = "other-proxy.test".into();
        preserve_global_proxy_reference(&local, &mut incoming);
        let Some(proxy) = global_proxy(&incoming) else {
            panic!("proxy config");
        };
        assert!(matches!(
            proxy.auth,
            SavedUpstreamProxyAuth::Password {
                keychain_id: None,
                ..
            }
        ));
    }

    #[test]
    fn network_settings_export_omits_device_local_password_reference() {
        let snapshot = oxideterm_settings::export_oxide_settings_snapshot_json(
            &settings("device-secret-reference"),
            Some(&std::collections::HashSet::from(["network".into()])),
            false,
        )
        .unwrap();
        assert!(!snapshot.contains("device-secret-reference"));
        assert!(snapshot.contains("proxy.test"));
    }
}
