use std::{result::Result as StdResult, sync::mpsc};

use gpui::{Context, Window};
use oxideterm_ssh::{AuthMethod, HostKeyStatus, SshConfig, SshTransportClient, check_host_key};

use super::{
    form_state::{NewConnectionForm, SshAuthTab},
    host_key_dialog::HostKeyChallenge,
};
use crate::workspace::WorkspaceApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshConnectionIntent {
    Test,
    Connect,
}

pub(in crate::workspace) enum SshConnectionWorkerResult {
    Preflight {
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        status: HostKeyStatus,
    },
    Test {
        result: StdResult<(), String>,
    },
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_connection_form = Some(NewConnectionForm {
            group: self.i18n.t("ssh.form.ungrouped"),
            ..NewConnectionForm::default()
        });
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(in crate::workspace) fn close_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_connection_form = None;
        self.host_key_challenge = None;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn submit_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_new_connection_flow(SshConnectionIntent::Connect, window, cx);
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((config, title)) = self.build_new_connection_config(cx) else {
            return;
        };
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        }

        let (tx, rx) = mpsc::channel();
        self.ssh_worker_rx = Some(rx);
        let host = config.host.clone();
        let port = config.port;
        let worker_config = config.clone();
        let worker_title = title.clone();
        std::thread::spawn(move || {
            let status = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(check_host_key(&host, port, 10)),
                Err(error) => HostKeyStatus::Error {
                    message: format!("failed to initialize SSH runtime: {error}"),
                },
            };
            let _ = tx.send(SshConnectionWorkerResult::Preflight {
                config: worker_config,
                title: worker_title,
                intent,
                status,
            });
        });
        cx.notify();
    }

    fn build_new_connection_config(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(SshConfig, String)> {
        let Some(form) = self.new_connection_form.as_mut() else {
            return None;
        };
        let host = form.host.trim().to_string();
        let username = form.username.trim().to_string();
        let port = form.port.trim().parse::<u16>().ok();
        if host.is_empty() || username.is_empty() || port.is_none() {
            form.error = Some(self.i18n.t("ssh.form.validation_required"));
            cx.notify();
            return None;
        }

        let auth = match form.auth_tab {
            SshAuthTab::Password => AuthMethod::password(form.password.clone()),
            SshAuthTab::Agent => AuthMethod::Agent,
            SshAuthTab::DefaultKey => AuthMethod::key("", None),
            SshAuthTab::SshKey => {
                form.error = Some(self.i18n.t("ssh.form.key_path_not_ready"));
                cx.notify();
                return None;
            }
            SshAuthTab::Certificate => {
                form.error = Some(self.i18n.t("ssh.form.certificate_not_ready"));
                cx.notify();
                return None;
            }
            SshAuthTab::TwoFactor => {
                form.error = Some(self.i18n.t("ssh.form.keyboard_interactive_not_ready"));
                cx.notify();
                return None;
            }
        };
        let config = SshConfig {
            host: host.clone(),
            port: port.unwrap_or(22),
            username: username.clone(),
            auth,
            agent_forwarding: form.agent_forwarding,
            strict_host_key_checking: true,
            ..SshConfig::default()
        };
        let title = if form.name.trim().is_empty() {
            format!("{username}@{host}")
        } else {
            form.name.trim().to_string()
        };
        Some((config, title))
    }

    pub(in crate::workspace) fn poll_ssh_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut results = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = self.ssh_worker_rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(result) => results.push(result),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected || !results.is_empty() {
            self.ssh_worker_rx = None;
        }

        for result in results {
            match result {
                SshConnectionWorkerResult::Preflight {
                    config,
                    title,
                    intent,
                    status,
                } => self.handle_ssh_preflight_result(config, title, intent, status, window, cx),
                SshConnectionWorkerResult::Test { result } => {
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(match result {
                            Ok(()) => self.i18n.t("ssh.form.test_success"),
                            Err(error) => error,
                        });
                    }
                    cx.notify();
                }
            }
        }
    }

    fn handle_ssh_preflight_result(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        status: HostKeyStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = None;
        }

        match status {
            HostKeyStatus::Verified => {
                self.continue_verified_ssh_flow(config, title, intent, window, cx)
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                self.host_key_challenge = Some(HostKeyChallenge {
                    config,
                    title,
                    status,
                    intent,
                });
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(message);
                }
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn continue_verified_ssh_flow(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match intent {
            SshConnectionIntent::Connect => {
                self.new_connection_form = None;
                self.host_key_challenge = None;
                let _ = self.create_ssh_terminal_tab(config, title, window, cx);
            }
            SshConnectionIntent::Test => self.start_ssh_test(config, cx),
        }
    }

    fn start_ssh_test(&mut self, config: SshConfig, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.test_running"));
        }
        let (tx, rx) = mpsc::channel();
        self.ssh_worker_rx = Some(rx);
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime
                    .block_on(SshTransportClient::new(config).test_connection())
                    .map_err(|error| error.to_string()),
                Err(error) => Err(format!("failed to initialize SSH runtime: {error}")),
            };
            let _ = tx.send(SshConnectionWorkerResult::Test { result });
        });
        cx.notify();
    }
}
