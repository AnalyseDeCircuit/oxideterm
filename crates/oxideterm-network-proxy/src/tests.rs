// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_settings::{
    PersistedSettings, SettingsApplicationProxyMode, SettingsUpstreamProxyAuth,
    SettingsUpstreamProxyConfig, SettingsUpstreamProxyProtocol, UpdateProxyMode,
    UpdateProxyProtocol, UpdateProxySettings,
};
use zeroize::Zeroizing;

use super::*;
use crate::{http::configure_http_client_builder, http::proxy_url};

struct TestCredentials {
    password: Option<&'static str>,
}

impl ApplicationProxyCredentialProvider for TestCredentials {
    fn application_proxy_password(
        &self,
        _keychain_id: &str,
    ) -> Result<Zeroizing<String>, ApplicationProxyError> {
        self.password
            .map(|password| Zeroizing::new(password.to_string()))
            .ok_or_else(|| {
                ApplicationProxyError::Unavailable("test credential is unavailable".to_string())
            })
    }
}

fn custom_proxy(protocol: ApplicationProxyProtocol) -> CustomApplicationProxy {
    CustomApplicationProxy {
        protocol,
        host: "127.0.0.1".to_string(),
        port: 1080,
        auth: ApplicationProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    }
}

#[test]
fn proxy_debug_redacts_password() {
    let policy = ApplicationProxyPolicy::Custom(CustomApplicationProxy {
        auth: ApplicationProxyAuth::Password {
            username: "proxy-user".to_string(),
            password: Zeroizing::new("proxy-secret".to_string()),
        },
        ..custom_proxy(ApplicationProxyProtocol::Socks5)
    });

    let debug = format!("{policy:?}");
    assert!(debug.contains("proxy-user"));
    assert!(!debug.contains("proxy-secret"));
}

#[test]
fn proxy_url_brackets_ipv6_and_preserves_dns_mode() {
    let config = CustomApplicationProxy {
        host: "::1".to_string(),
        ..custom_proxy(ApplicationProxyProtocol::Socks5)
    };

    assert_eq!(proxy_url(&config).unwrap(), "socks5h://[::1]:1080");
}

#[test]
fn update_proxy_protocols_use_the_expected_schemes() {
    assert_eq!(
        proxy_url(&custom_proxy(ApplicationProxyProtocol::HttpsConnect)).unwrap(),
        "https://127.0.0.1:1080"
    );
    assert_eq!(
        proxy_url(&custom_proxy(ApplicationProxyProtocol::Socks5)).unwrap(),
        "socks5h://127.0.0.1:1080"
    );
}

#[test]
fn unavailable_policy_fails_before_building_a_client() {
    let error = configure_http_client_builder(
        reqwest::Client::builder(),
        &ApplicationProxyPolicy::Unavailable {
            reason: "password is unavailable".to_string(),
        },
    )
    .expect_err("an unavailable proxy must not fall back to a direct client");

    assert!(error.to_string().contains("password is unavailable"));
}

#[test]
fn custom_policy_rejects_an_empty_host() {
    let policy = ApplicationProxyPolicy::Custom(CustomApplicationProxy {
        host: "  ".to_string(),
        ..custom_proxy(ApplicationProxyProtocol::HttpConnect)
    });

    assert!(configure_http_client_builder(reqwest::Client::builder(), &policy).is_err());
}

#[test]
fn settings_hydrate_application_proxy_credentials() {
    let mut settings = PersistedSettings::default();
    settings.network.application_proxy_mode = SettingsApplicationProxyMode::Shared;
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "proxy.example".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::Password {
            username: "proxy-user".to_string(),
            keychain_id: Some("credential-id".to_string()),
        },
        remote_dns: true,
        no_proxy: "localhost".to_string(),
    });

    let policy = application_proxy_policy_from_settings(
        &settings,
        &TestCredentials {
            password: Some("proxy-secret"),
        },
    );
    let debug = format!("{policy:?}");
    assert!(matches!(policy, ApplicationProxyPolicy::Custom(_)));
    assert!(!debug.contains("proxy-secret"));
}

#[test]
fn missing_application_proxy_password_is_fail_closed() {
    let mut settings = PersistedSettings::default();
    settings.network.application_proxy_mode = SettingsApplicationProxyMode::Shared;
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::HttpConnect,
        host: "proxy.example".to_string(),
        port: 8080,
        auth: SettingsUpstreamProxyAuth::Password {
            username: "proxy-user".to_string(),
            keychain_id: Some("credential-id".to_string()),
        },
        remote_dns: true,
        no_proxy: String::new(),
    });

    assert!(matches!(
        application_proxy_policy_from_settings(&settings, &TestCredentials { password: None }),
        ApplicationProxyPolicy::Unavailable { .. }
    ));
}

#[test]
fn default_application_proxy_mode_preserves_system_policy() {
    assert_eq!(
        application_proxy_policy_from_settings(
            &PersistedSettings::default(),
            &TestCredentials { password: None }
        ),
        ApplicationProxyPolicy::System
    );
}

#[test]
fn direct_application_proxy_mode_disables_system_proxy_discovery() {
    let mut settings = PersistedSettings::default();
    settings.network.application_proxy_mode = SettingsApplicationProxyMode::Direct;

    assert_eq!(
        application_proxy_policy_from_settings(&settings, &TestCredentials { password: None }),
        ApplicationProxyPolicy::Direct
    );
}

#[test]
fn custom_update_proxy_is_configured_by_the_shared_adapter() {
    let settings = UpdateProxySettings {
        mode: UpdateProxyMode::Custom,
        protocol: UpdateProxyProtocol::Socks5,
        host: "127.0.0.1".to_string(),
        port: 7890,
        ..UpdateProxySettings::default()
    };

    assert!(configure_update_http_client_builder(reqwest::Client::builder(), &settings).is_ok());
}

#[test]
fn custom_update_proxy_rejects_an_empty_host() {
    let settings = UpdateProxySettings {
        mode: UpdateProxyMode::Custom,
        host: "  ".to_string(),
        ..UpdateProxySettings::default()
    };

    assert!(configure_update_http_client_builder(reqwest::Client::builder(), &settings).is_err());
}

#[test]
fn replacing_runtime_policy_replaces_the_pooled_client_state() {
    set_application_proxy_policy(ApplicationProxyPolicy::Unavailable {
        reason: "test proxy is unavailable".to_string(),
    });
    assert!(application_http_client().is_err());

    // Restore the process default so this test does not affect later tests.
    set_application_proxy_policy(ApplicationProxyPolicy::System);
    assert!(application_http_client().is_ok());
}
