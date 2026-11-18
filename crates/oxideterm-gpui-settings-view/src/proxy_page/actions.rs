use std::sync::Arc;

use gpui::Context;
use oxideterm_connections::ConnectionStore;
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    PersistedSettings, SettingsApplicationProxyMode, SettingsUpstreamProxyAuth,
    SettingsUpstreamProxyConfig,
};
use oxideterm_ssh::{HostKeyStatus, UpstreamProxyConfig, probe_upstream_proxy_route};

use super::{NetworkProxyAuthMode, ProxyRequest, default_settings_upstream_proxy_config};
use crate::{components::SettingsEdit, entity::SettingsWorkspaceEntity};

impl SettingsWorkspaceEntity {
    pub fn apply_proxy_request(
        &mut self,
        request: ProxyRequest,
        persisted: &PersistedSettings,
        store: &mut ConnectionStore,
        i18n: &I18n,
        runtime: Arc<tokio::runtime::Runtime>,
        resolve_proxy: impl FnOnce(
            &ConnectionStore,
            &SettingsUpstreamProxyConfig,
        ) -> Option<UpstreamProxyConfig>,
        cx: &mut Context<Self>,
    ) -> Option<SettingsEdit> {
        let proxy = persisted.network.upstream_proxy.as_ref();
        let has_saved_password = proxy.is_some_and(|proxy| {
            matches!(
                proxy.auth,
                SettingsUpstreamProxyAuth::Password {
                    keychain_id: Some(_),
                    ..
                }
            )
        });
        match request {
            ProxyRequest::Disclaimer => Some(Box::new(|settings| {
                settings.network.upstream_proxy_disclaimer_accepted =
                    !settings.network.upstream_proxy_disclaimer_accepted;
            })),
            ProxyRequest::Enabled => {
                self.set_network_proxy_password_status(None, cx);
                if has_saved_password
                    && let Err(error) = store.delete_global_upstream_proxy_password()
                {
                    self.set_network_proxy_password_status(Some(error.to_string()), cx);
                    return None;
                }
                self.finish_network_proxy_password_action(None, cx);
                Some(Box::new(|settings| {
                    if settings.network.upstream_proxy.is_some() {
                        settings.network.application_proxy_mode =
                            SettingsApplicationProxyMode::System;
                        settings.network.upstream_proxy = None;
                    } else {
                        settings.network.upstream_proxy =
                            Some(default_settings_upstream_proxy_config());
                    }
                }))
            }
            ProxyRequest::RemoteDns => Some(Box::new(|settings| {
                if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                    proxy.remote_dns = !proxy.remote_dns;
                }
            })),
            ProxyRequest::Auth(mode) => {
                self.finish_network_proxy_password_action(None, cx);
                if mode == NetworkProxyAuthMode::None
                    && has_saved_password
                    && let Err(error) = store.delete_global_upstream_proxy_password()
                {
                    self.set_network_proxy_password_status(Some(error.to_string()), cx);
                    return None;
                }
                Some(Box::new(move |settings| {
                    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                        proxy.auth = match mode {
                            NetworkProxyAuthMode::None => SettingsUpstreamProxyAuth::None,
                            NetworkProxyAuthMode::Password => SettingsUpstreamProxyAuth::Password {
                                username: String::new(),
                                keychain_id: None,
                            },
                        };
                    }
                }))
            }
            ProxyRequest::Test => {
                let Some(proxy) = proxy else {
                    self.set_network_proxy_test_error("proxy is disabled".to_string(), cx);
                    return None;
                };
                let Some(upstream) = resolve_proxy(store, proxy) else {
                    self.set_network_proxy_test_error(
                        "proxy password is not available".to_string(),
                        cx,
                    );
                    return None;
                };
                self.start_network_proxy_test(
                    runtime,
                    upstream,
                    schedule_settings_network_proxy_test,
                    cx,
                );
                None
            }
            ProxyRequest::SavePassword => {
                let secret = self.take_network_proxy_password()?;
                match store.save_global_upstream_proxy_password(&secret) {
                    Ok(keychain_id) => {
                        self.finish_network_proxy_password_action(
                            Some(i18n.t("settings_view.network.password_saved_placeholder")),
                            cx,
                        );
                        Some(proxy_keychain_edit(Some(keychain_id)))
                    }
                    Err(error) => {
                        self.restore_network_proxy_password(secret, error.to_string(), cx);
                        None
                    }
                }
            }
            ProxyRequest::RemovePassword => match store.delete_global_upstream_proxy_password() {
                Ok(()) => {
                    self.finish_network_proxy_password_action(None, cx);
                    Some(proxy_keychain_edit(None))
                }
                Err(error) => {
                    self.set_network_proxy_password_status(Some(error.to_string()), cx);
                    None
                }
            },
        }
    }
}

fn proxy_keychain_edit(keychain_id: Option<String>) -> SettingsEdit {
    Box::new(move |settings| {
        if let Some(proxy) = settings.network.upstream_proxy.as_mut()
            && let SettingsUpstreamProxyAuth::Password {
                keychain_id: saved, ..
            } = &mut proxy.auth
        {
            *saved = keychain_id;
        }
    })
}

fn schedule_settings_network_proxy_test(
    runtime: &tokio::runtime::Runtime,
    host: String,
    port: u16,
    upstream_proxy: UpstreamProxyConfig,
) -> tokio::task::JoinHandle<HostKeyStatus> {
    // The workspace runtime owns network I/O; the GPUI task receives only the
    // resulting status and never polls Tokio sockets on the UI executor.
    runtime.spawn(async move {
        match probe_upstream_proxy_route(&host, port, 10, &upstream_proxy).await {
            Ok(()) => HostKeyStatus::Verified,
            Err(error) => HostKeyStatus::Error {
                message: error.to_string(),
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SettingsInput;
    use gpui::{AppContext, TestAppContext};
    use oxideterm_ssh::{UpstreamProxyAuth, UpstreamProxyProtocol};

    #[gpui::test]
    fn proxy_password_moves_and_restores_without_plaintext_clone(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            assert!(settings.focus_settings_entity_input(SettingsInput::NetworkProxyPassword, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::NetworkProxyPassword,
                None,
                "proxy-secret",
                cx,
            ));
            let draft_allocation = settings.network_proxy_password.as_ptr();

            let password = settings
                .take_network_proxy_password()
                .expect("network proxy password");
            assert_eq!(password.expose_secret(), "proxy-secret");
            assert_eq!(password.expose_secret().as_ptr(), draft_allocation);
            assert!(settings.network_proxy_password.is_empty());

            settings.restore_network_proxy_password(password, "retry".to_string(), cx);
            assert_eq!(settings.network_proxy_password.as_str(), "proxy-secret");
            assert_eq!(
                settings.network_proxy_password_status.as_deref(),
                Some("retry")
            );

            settings.finish_network_proxy_password_action(None, cx);
            assert!(settings.network_proxy_password.is_empty());
            assert_eq!(settings.settings_entity_focused_input(), None);
        });
    }

    #[gpui::test]
    fn proxy_route_test_task_and_completion_are_entity_owned(cx: &mut TestAppContext) {
        let proxy_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("bind an entity proxy test listener");
        let proxy_port = proxy_listener
            .local_addr()
            .expect("read entity proxy address")
            .port();
        let proxy_server = std::thread::spawn(move || {
            let (stream, _) = proxy_listener
                .accept()
                .expect("accept entity proxy test client");
            drop(stream);
        });
        let runtime = std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("create entity proxy runtime"),
        );
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.network_proxy_test_host = "proxy-test-target.invalid".to_string();
            settings.network_proxy_test_port = "22".to_string();
            assert!(settings.start_network_proxy_test(
                runtime,
                UpstreamProxyConfig {
                    protocol: UpstreamProxyProtocol::Socks5,
                    host: std::net::Ipv4Addr::LOCALHOST.to_string(),
                    port: proxy_port,
                    auth: UpstreamProxyAuth::None,
                    remote_dns: true,
                    no_proxy: String::new(),
                },
                schedule_settings_network_proxy_test,
                cx,
            ));
            assert!(settings.network_proxy_test_pending);
            assert!(settings.network_proxy_test_task.is_some());
            assert!(settings.network_proxy_test_abort.is_some());
        });

        proxy_server.join().expect("join entity proxy test server");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while settings.read_with(cx, |settings, _cx| settings.network_proxy_test_pending)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        settings.update(cx, |settings, _cx| {
            assert!(!settings.network_proxy_test_pending);
            assert!(settings.network_proxy_test_task.is_none());
            assert!(settings.network_proxy_test_abort.is_none());
            assert!(matches!(settings.network_proxy_test_result, Some(Err(_))));
        });
    }
}
