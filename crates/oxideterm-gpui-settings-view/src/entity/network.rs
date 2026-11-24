use super::*;
use oxideterm_connections::SecretString;
use oxideterm_ssh::{HostKeyStatus, UpstreamProxyConfig};

impl SettingsWorkspaceEntity {
    pub fn network_proxy_layout_flags(&self) -> (bool, bool, bool) {
        (
            self.network_proxy_password_status.is_some(),
            self.network_proxy_test_pending,
            self.network_proxy_test_result.is_some(),
        )
    }

    pub fn network_proxy_password_snapshot(&self) -> NetworkProxyPasswordSnapshot {
        NetworkProxyPasswordSnapshot {
            // The input control borrows plaintext directly from the Entity.
            password_present: !self.network_proxy_password.is_empty(),
            password_status: self.network_proxy_password_status.clone(),
        }
    }

    pub fn network_proxy_test_snapshot(&self) -> NetworkProxyTestSnapshot {
        NetworkProxyTestSnapshot {
            test_host: self.network_proxy_test_host.clone(),
            test_port: self.network_proxy_test_port.clone(),
            test_pending: self.network_proxy_test_pending,
            test_result: self.network_proxy_test_result.clone(),
        }
    }

    pub fn take_network_proxy_password(&mut self) -> Option<SecretString> {
        if self.network_proxy_password.is_empty() {
            return None;
        }
        self.settings_focused_input = None;
        let password = std::mem::replace(
            &mut self.network_proxy_password,
            zeroize::Zeroizing::new(String::new()),
        );
        Some(SecretString::from(password))
    }

    pub fn restore_network_proxy_password(
        &mut self,
        password: SecretString,
        status: String,
        cx: &mut Context<Self>,
    ) {
        zeroize::Zeroize::zeroize(&mut *self.network_proxy_password);
        self.network_proxy_password = password.into_zeroizing();
        self.network_proxy_password_status = Some(status);
        cx.notify();
    }

    pub fn finish_network_proxy_password_action(
        &mut self,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        zeroize::Zeroize::zeroize(&mut *self.network_proxy_password);
        if self.settings_focused_input == Some(SettingsInput::NetworkProxyPassword) {
            self.settings_focused_input = None;
        }
        self.network_proxy_password_status = status;
        cx.notify();
    }

    pub fn set_network_proxy_password_status(
        &mut self,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.network_proxy_password_status = status;
        cx.notify();
    }

    pub fn set_network_proxy_test_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.network_proxy_test_result = Some(Err(error));
        cx.notify();
    }

    pub fn start_network_proxy_test(
        &mut self,
        runtime: std::sync::Arc<tokio::runtime::Runtime>,
        upstream_proxy: UpstreamProxyConfig,
        schedule: fn(
            &tokio::runtime::Runtime,
            String,
            u16,
            UpstreamProxyConfig,
        ) -> tokio::task::JoinHandle<HostKeyStatus>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.network_proxy_test_pending {
            return false;
        }
        let host = self.network_proxy_test_host.trim().to_string();
        if host.is_empty() {
            self.set_network_proxy_test_error("host is required".to_string(), cx);
            return false;
        }
        let Ok(port) = self.network_proxy_test_port.trim().parse::<u16>() else {
            self.set_network_proxy_test_error("invalid port".to_string(), cx);
            return false;
        };

        if let Some(abort) = self.network_proxy_test_abort.take() {
            abort.abort();
        }
        let started_at = tokio::time::Instant::now();
        let worker = schedule(runtime.as_ref(), host, port, upstream_proxy);
        self.network_proxy_test_abort = Some(worker.abort_handle());
        self.network_proxy_test_pending = true;
        self.network_proxy_test_result = None;
        self.network_proxy_test_task = Some(cx.spawn(async move |settings, cx| {
            // Keep the shared runtime alive until this Entity-owned worker completes.
            let _runtime_owner = runtime;
            let status = worker.await.unwrap_or_else(|_| HostKeyStatus::Error {
                message: "proxy route test task stopped unexpectedly".to_string(),
            });
            let elapsed = started_at.elapsed().as_millis();
            let _ = settings.update(cx, |settings, cx| {
                settings.network_proxy_test_task = None;
                settings.network_proxy_test_abort = None;
                settings.network_proxy_test_pending = false;
                settings.network_proxy_test_result = Some(match status {
                    HostKeyStatus::Error { message } => Err(message),
                    _ => Ok(elapsed),
                });
                cx.notify();
            });
        }));
        cx.notify();
        true
    }
}
