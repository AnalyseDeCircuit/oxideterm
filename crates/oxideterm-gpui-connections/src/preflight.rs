// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_connections::ConnectionStore;
use oxideterm_i18n::I18n;
use oxideterm_session_adapter::managed_key_resolver_from_store;
use oxideterm_settings::PersistedSettings;
use oxideterm_ssh::{
    HostKeyStatus, NativeSessionTreeConnectStep, NodeRouter, SshConfig, UpstreamProxyConfig,
};

use crate::{
    configuration::RuntimeSecretHandoff,
    conversion::{build_ssh_config, prepare_tree_connect_config},
    entity::{ConnectionFlowEntity, ProxyConnectPreflightContext},
    launch::SshConnectionIntent,
    prompts::HostKeyChallenge,
};

pub enum RoutePreflight {
    Test,
    Routed,
    Direct,
    AlreadyVerified,
}

pub struct SshPreflightPlan {
    pub route: RoutePreflight,
    pub config: SshConfig,
    pub title: String,
    pub intent: SshConnectionIntent,
}

pub enum PreflightOutcome {
    Continue(SshConfig, String, SshConnectionIntent),
    ConfirmHostKey,
    Failed {
        intent: SshConnectionIntent,
        message: String,
        inline: bool,
    },
}

pub enum TreePreflightOutcome {
    Stale,
    Advance,
    ConfirmHostKey,
    Failed(String),
}

pub enum VerifiedManualSshRoute {
    Direct,
    SessionTree,
}

pub enum SshTestRoute {
    Direct,
    HostKeyPreflight,
}

impl crate::views::ConnectionsViews {
    pub fn prepare_ssh_test_route(
        &self,
        config: &mut SshConfig,
        i18n: &I18n,
        cx: &mut gpui::App,
    ) -> SshTestRoute {
        if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            crate::conversion::prepare_proxy_chain_test_config(config);
            return SshTestRoute::Direct;
        }
        self.report_connection_feedback(true, i18n.t("ssh.form.checking_host_key"), cx);
        SshTestRoute::HostKeyPreflight
    }

    pub fn finish_verified_saved_form(&self, cx: &mut gpui::App) {
        self.flow.update(cx, |flow, cx| {
            flow.clear_host_key_challenge(cx);
            if flow.form.form.is_some() {
                flow.clear_connected_form(cx);
            }
        });
        self.session_manager
            .update(cx, |manager, cx| manager.set_status(None, cx));
    }

    pub fn finish_drill_down_form(&self, i18n: &I18n, cx: &mut gpui::App) {
        self.flow
            .update(cx, ConnectionFlowEntity::clear_connected_form);
        self.show_catalog_status(i18n.t("ssh.drill_down.connecting"), cx);
    }
}

pub struct ExistingSftpSurface {
    pub endpoint_id: String,
    pub title: String,
    pub path: Option<String>,
}

impl ConnectionFlowEntity {
    pub fn prepare_verified_manual_terminal(
        &mut self,
        config: &SshConfig,
        cx: &mut Context<Self>,
    ) -> VerifiedManualSshRoute {
        self.clear_connected_form(cx);
        self.clear_host_key_challenge(cx);
        if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            VerifiedManualSshRoute::SessionTree
        } else {
            VerifiedManualSshRoute::Direct
        }
    }

    pub(crate) fn clear_connected_form(&mut self, cx: &mut Context<Self>) {
        self.form.clear();
        cx.notify();
    }

    pub fn start_standalone_sftp_test(
        &mut self,
        config: SshConfig,
        store: &ConnectionStore,
        registry: oxideterm_ssh::SshConnectionRegistry,
        runtime: &tokio::runtime::Runtime,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        self.set_form_feedback(Some(true), Some(i18n.t("ssh.form.test_running")), cx);
        crate::workers::test_standalone_sftp(
            config,
            self.ssh_worker_sender(),
            managed_key_resolver_from_store(store),
            registry,
            runtime,
        );
    }

    pub fn open_verified_sftp(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        existing_endpoint: impl FnOnce(&str) -> bool,
        store: &ConnectionStore,
        registry: oxideterm_ssh::SshConnectionRegistry,
        runtime: &tokio::runtime::Runtime,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<ExistingSftpSurface> {
        self.clear_host_key_challenge(cx);
        match intent {
            SshConnectionIntent::StandaloneSftp { pair_launch_token: Some(token), .. } => {
                if let Some(config) = self.accept_sftp_pair_primary(&token, config) {
                    crate::workers::preflight(
                        config, title, SshConnectionIntent::StandaloneSftpSecondary { pair_launch_token: token },
                        self.ssh_worker_sender(), managed_key_resolver_from_store(store),
                        i18n.clone(),
                    );
                } else {
                    self.take_sftp_pair_launch(&token);
                    self.set_form_feedback(None, Some(i18n.t("sftp.standalone.missing_credentials")), cx);
                }
            }
            SshConnectionIntent::StandaloneSftp { saved_profile_id, initial_remote_path, pair_launch_token: None } => {
                let endpoint_id = saved_profile_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if existing_endpoint(&endpoint_id) {
                    self.clear_form_after_submit(cx);
                    return Some(ExistingSftpSurface { endpoint_id, title, path: initial_remote_path });
                }
                crate::workers::connect_standalone_sftp(
                    config, endpoint_id, saved_profile_id, title, initial_remote_path,
                    self.ssh_worker_sender(), managed_key_resolver_from_store(store), registry, runtime,
                );
            }
            SshConnectionIntent::StandaloneSftpSecondary { pair_launch_token } => {
                let mut launch = self.take_sftp_pair_launch(&pair_launch_token)?;
                let primary = launch.primary_config.take()?;
                crate::workers::connect_standalone_sftp_pair(
                    primary, config, launch, self.ssh_worker_sender(),
                    managed_key_resolver_from_store(store), registry, runtime,
                );
            }
            _ => unreachable!("SSH launch must be a standalone SFTP endpoint"),
        }
        None
    }
}

impl ConnectionFlowEntity {
    pub fn plan_form_ssh_preflight(
        &mut self,
        store: &ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        intent: &SshConnectionIntent,
        cx: &mut Context<Self>,
    ) -> Option<(SshConfig, String)> {
        let form = self.form.form.as_mut()?;
        let testing = matches!(
            intent,
            SshConnectionIntent::Test | SshConnectionIntent::TestStandaloneSftp
        );
        if testing && form.auth_tab == crate::SshAuthTab::TwoFactor {
            self.set_form_feedback(None, Some(i18n.t("ssh.form.test_not_supported_kbi")), cx);
            return None;
        }
        let handoff = if testing {
            RuntimeSecretHandoff::CopyForTest
        } else {
            RuntimeSecretHandoff::Move
        };
        match build_ssh_config(form, store, settings, i18n, handoff) {
            Ok(value) => Some(value),
            Err(error) => {
                self.set_form_feedback(None, Some(error), cx);
                None
            }
        }
    }

    pub fn plan_preflight(
        &mut self,
        mut config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        form_feedback: bool,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Result<SshPreflightPlan, Option<String>> {
        let title = ssh_connection_display_title(&title);
        let route = if intent == SshConnectionIntent::Test {
            RoutePreflight::Test
        } else {
            if let Err(message) = prepare_tree_connect_config(&mut config) {
                let reported = self.set_form_feedback(None, Some(message.clone()), cx);
                return Err((!reported).then_some(message));
            }
            if matches!(&intent, SshConnectionIntent::DrillDown { .. }) {
                // Only the live parent may be able to reach the target.
                RoutePreflight::AlreadyVerified
            } else {
                if form_feedback {
                    self.set_form_feedback(
                        Some(true),
                        Some(i18n.t("ssh.form.checking_host_key")),
                        cx,
                    );
                }
                if config.proxy_chain.is_some()
                    && !matches!(
                        intent,
                        SshConnectionIntent::StandaloneSftp { .. }
                            | SshConnectionIntent::StandaloneSftpSecondary { .. }
                            | SshConnectionIntent::TestStandaloneSftp
                    )
                {
                    RoutePreflight::Routed
                } else {
                    RoutePreflight::Direct
                }
            }
        };
        Ok(SshPreflightPlan {
            route,
            config,
            title,
            intent,
        })
    }

    pub fn resolve_preflight(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        host: String,
        port: u16,
        status: HostKeyStatus,
        cx: &mut Context<Self>,
    ) -> PreflightOutcome {
        self.set_form_feedback(Some(false), None, cx);
        match status {
            HostKeyStatus::Verified => PreflightOutcome::Continue(config, title, intent),
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                self.open_host_key_challenge(
                    HostKeyChallenge {
                        presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
                        config,
                        title,
                        status,
                        intent,
                        session_tree_challenge: false,
                        host,
                        port,
                    },
                    cx,
                );
                PreflightOutcome::ConfirmHostKey
            }
            HostKeyStatus::Error { message } => {
                if let Some(token) = intent.standalone_sftp_pair_launch_token() {
                    self.take_sftp_pair_launch(token);
                }
                let inline = self.set_form_feedback(None, Some(message.clone()), cx);
                PreflightOutcome::Failed {
                    intent,
                    message,
                    inline,
                }
            }
        }
    }

    pub fn resolve_tree_preflight(
        &mut self,
        preflight: ProxyConnectPreflightContext,
        status: HostKeyStatus,
        cx: &mut Context<Self>,
    ) -> TreePreflightOutcome {
        if !self.restore_proxy_connect_preflight_context(preflight, cx) {
            return TreePreflightOutcome::Stale;
        }
        self.set_form_feedback(Some(false), None, cx);
        match status {
            HostKeyStatus::Verified => match self.mark_current_proxy_connect_step_verified(cx) {
                Ok(()) => TreePreflightOutcome::Advance,
                Err(message) => TreePreflightOutcome::Failed(message),
            },
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                if self.open_active_proxy_host_key_challenge(status, cx) {
                    TreePreflightOutcome::ConfirmHostKey
                } else {
                    TreePreflightOutcome::Stale
                }
            }
            HostKeyStatus::Error { message } => TreePreflightOutcome::Failed(message),
        }
    }

    pub fn start_connection_test(
        &mut self,
        config: SshConfig,
        store: &ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let message = i18n.t("ssh.form.test_running");
        let inline = self.set_form_feedback(Some(true), Some(message.clone()), cx);
        crate::workers::test_ssh(
            config,
            self.ssh_worker_sender(),
            managed_key_resolver_from_store(store),
        );
        (!inline).then_some(message)
    }

    pub fn finish_connection_test(
        &mut self,
        result: Result<(), String>,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        match result {
            Ok(()) => (!self.set_form_success_feedback(i18n.t("ssh.form.test_success"), cx))
                .then(|| i18n.t("sessionManager.toast.test_success")),
            Err(error) => {
                let status = format!("{}: {error}", i18n.t("sessionManager.toast.test_failed"));
                (!self.set_form_feedback(Some(false), Some(error), cx)).then_some(status)
            }
        }
    }
}

pub fn ssh_connection_display_title(name: &str) -> String {
    // Keep tabs out of single-line session titles without changing the saved SSH alias.
    name.replace('\t', " ")
}

pub fn apply_tree_step_preflight(
    router: &NodeRouter,
    step: &NativeSessionTreeConnectStep,
    proxy: Option<UpstreamProxyConfig>,
) -> bool {
    if step.trust_host_key.is_none() && !step.preflight_verified && proxy.is_none() {
        return false;
    }
    let Some(snapshot) = router.node_runtime_snapshot(&step.node_id) else {
        return false;
    };
    let mut config = snapshot.config;
    if let Some(proxy) = proxy {
        config.upstream_proxy = Some(proxy);
    }
    if let (Some(accept), Some(fingerprint)) = (
        step.trust_host_key,
        step.expected_host_key_fingerprint.clone(),
    ) {
        config.strict_host_key_checking = true;
        config.trust_host_key = Some(accept);
        config.expected_host_key_fingerprint = Some(fingerprint);
        config.host_key_replacement = step.host_key_replacement.clone();
    } else if step.preflight_verified {
        // Fresh verification replaces the decision on any earlier attempt.
        config.strict_host_key_checking = true;
        config.trust_host_key = None;
        config.expected_host_key_fingerprint = None;
        config.host_key_replacement = None;
    }
    router.upsert_node_with_origin(step.node_id.clone(), config, snapshot.origin);
    true
}
