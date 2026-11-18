use super::*;
use oxideterm_gpui_settings_view::mcp_page::{
    McpClientDialog, McpSettingsFrame, McpSettingsPage, McpSettingsRequest,
};
use oxideterm_gpui_settings_view::proxy_page::{ProxyPage, ProxyRequest};

#[derive(serde::Serialize)]
struct PublicMcpStdioEnvironment<'a> {
    #[serde(rename = "OXIDETERM_MCP_TOKEN")]
    credential: &'a str,
}

#[derive(serde::Serialize)]
struct PublicMcpStdioConfig<'a> {
    command: &'static str,
    args: [&'static str; 2],
    env: PublicMcpStdioEnvironment<'a>,
}

fn public_mcp_stdio_json(credential: &str) -> zeroize::Zeroizing<String> {
    // Borrow the credential during serialization so the JSON output is the only temporary copy.
    zeroize::Zeroizing::new(
        serde_json::to_string_pretty(&PublicMcpStdioConfig {
            command: CLI_COMPANION_COMMAND_NAME,
            args: ["mcp", "bridge"],
            env: PublicMcpStdioEnvironment { credential },
        })
        .expect("serializing the stdio MCP configuration cannot fail"),
    )
}

pub(in crate::workspace) use oxideterm_gpui_settings_view::proxy_page::{
    NetworkProxyAuthMode, default_settings_upstream_proxy_config, network_proxy_auth_label,
};

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

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_network_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match section {
            SettingsSectionId::NetworkSharedProxy | SettingsSectionId::NetworkRouting => {
                self.render_settings_proxy_section(section, cx.entity(), cx)
            }
            SettingsSectionId::NetworkPublicMcp => self.render_mcp_settings_page(cx.entity(), cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn open_public_mcp_client_editor(
        &mut self,
        client_ref: String,
        cx: &mut Context<Self>,
    ) {
        self.close_settings_select();
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.public_mcp_client_editor = Some(client_ref);
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_public_mcp_client_editor(&mut self, cx: &mut Context<Self>) {
        if self
            .settings_workspace
            .read(cx)
            .public_mcp_client_editor
            .is_none()
        {
            return;
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.public_mcp_client_editor = None;
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn mount_mcp_client_dialog(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<McpClientDialog> {
        let owner = cx.entity().downgrade();
        let body = Rc::new(move |cx: &App| {
            let owner = owner.upgrade()?;
            owner
                .read(cx)
                .with_mcp_settings_page(owner, cx, |page, cx| {
                    page.render_public_mcp_client_editor_modal(cx)
                })
        });
        cx.new(|cx| McpClientDialog::new(settings, body, cx))
    }

    fn with_mcp_settings_page<R>(
        &self,
        owner: Entity<Self>,
        cx: &App,
        render: impl FnOnce(McpSettingsPage<'_>, &App) -> R,
    ) -> R {
        let weak = owner.downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = weak.update(cx, |app, cx| app.handle_mcp_settings_request(request, cx));
        });
        let target_label = |client: &oxideterm_public_mcp::ClientRef, target: &str, cx: &App| {
            self.public_mcp_target_label(client, target, cx)
        };
        let scroll = self.selectable_text_scroll_handle("public-mcp-client-editor-scroll");
        self.with_settings_controls_for(owner, cx, |controls, cx| {
            render(
                McpSettingsPage {
                    controls,
                    state: &self.settings_workspace,
                    dispatch,
                    target_label: &target_label,
                    scroll: &scroll,
                    frame: McpSettingsFrame {
                        endpoint: self.public_mcp.endpoint_url(),
                        startup_error: self.public_mcp.startup_error(),
                        clients: self.public_mcp.clients(),
                        approvals: self.public_mcp.approvals(),
                        port_draft: self.public_mcp.port_draft(),
                        credential_ready: self.public_mcp.revealed_credential().is_some(),
                    },
                },
                cx,
            )
        })
    }

    pub(super) fn render_mcp_settings_page(&self, owner: Entity<Self>, cx: &App) -> AnyElement {
        self.with_mcp_settings_page(owner, cx, |page, cx| page.settings_public_mcp_section(cx))
    }

    pub(in crate::workspace) fn render_public_mcp_client_editor_modal(
        &self,
        cx: &App,
    ) -> Option<AnyElement> {
        self.settings_workspace
            .read(cx)
            .public_mcp_client_editor
            .is_some()
            .then(|| self.mcp_client_dialog.clone().into_any_element())
    }

    fn handle_mcp_settings_request(&mut self, request: McpSettingsRequest, cx: &mut Context<Self>) {
        match request {
            McpSettingsRequest::CreateClient => {
                let mode = self.settings_workspace.read(cx).new_mcp_client_mode;
                let label = format!(
                    "{} {}",
                    self.i18n.t("settings_view.network.external_client_name"),
                    self.public_mcp.clients().len() + 1
                );
                if let Err(error) = self.public_mcp.create_client(label, mode) {
                    self.public_mcp.record_action_error(error);
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::ApplyPort => {
                let draft = self.public_mcp.port_draft().trim().to_owned();
                let Ok(port) = draft.parse::<u16>() else {
                    let error = self.i18n.t("settings_view.network.invalid_public_mcp_port");
                    self.public_mcp.record_action_error(error);
                    self.settings_workspace.update(cx, |_, cx| cx.notify());
                    return;
                };
                let runtime = self.forwarding_runtime.handle().clone();
                if let Err(error) = self.public_mcp.apply_preferred_port(&runtime, port) {
                    self.public_mcp.record_action_error(error.to_string());
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::Approve(approval_ref_for_accept) => {
                if let Err(error) = self.public_mcp.set_approval_status(
                    &approval_ref_for_accept,
                    oxideterm_public_mcp::ApprovalStatus::Approved,
                ) {
                    self.public_mcp.record_action_error(error);
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::Reject(approval_ref_for_reject) => {
                if let Err(error) = self.public_mcp.set_approval_status(
                    &approval_ref_for_reject,
                    oxideterm_public_mcp::ApprovalStatus::Rejected,
                ) {
                    self.public_mcp.record_action_error(error);
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::CopyCredential => {
                if let Some(credential) = self.public_mcp.revealed_credential() {
                    cx.write_to_clipboard(ClipboardItem::new_string(credential.to_owned()));
                }
            }
            McpSettingsRequest::CopyStdio => {
                if let Some(credential) = self.public_mcp.revealed_credential() {
                    let mut config = public_mcp_stdio_json(credential);
                    // The clipboard is the explicit external boundary; app state never retains the JSON.
                    let clipboard_config = std::mem::take(&mut *config);
                    cx.write_to_clipboard(ClipboardItem::new_string(clipboard_config));
                }
            }
            McpSettingsRequest::DismissCredential => {
                self.public_mcp.dismiss_revealed_credential();
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::OpenEditor(client_ref_for_edit) => {
                self.open_public_mcp_client_editor(client_ref_for_edit, cx);
            }
            McpSettingsRequest::SetToolGroup(client_ref, tool_group, enabled) => {
                if let Err(error) =
                    self.set_public_mcp_client_tool_group(&client_ref, tool_group, enabled, cx)
                {
                    self.public_mcp.record_action_error(error);
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::SetMode(client_ref_for_mode, next_approval_mode) => {
                if let Err(error) = self.set_public_mcp_client_approval_mode(
                    &client_ref_for_mode,
                    next_approval_mode,
                    cx,
                ) {
                    self.public_mcp.record_action_error(error);
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::SetEnabled(client_ref_for_toggle, next_enabled) => {
                if let Err(error) =
                    self.set_public_mcp_client_enabled(&client_ref_for_toggle, next_enabled, cx)
                {
                    self.public_mcp.record_action_error(error);
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::RemoveClient(client_ref_for_remove) => {
                match self.remove_public_mcp_client(&client_ref_for_remove, cx) {
                    Ok(()) => self.close_public_mcp_client_editor(cx),
                    Err(error) => self.public_mcp.record_action_error(error),
                }
                self.settings_workspace.update(cx, |_, cx| cx.notify());
            }
            McpSettingsRequest::CloseEditor => {
                self.close_public_mcp_client_editor(cx);
            }
        }
    }

    pub(super) fn render_settings_proxy_section(
        &self,
        section: SettingsSectionId,
        owner: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let owner_weak = owner.downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = owner_weak.update(cx, |app, cx| match request {
                ProxyRequest::Disclaimer => app.toggle_settings_network_disclaimer(cx),
                ProxyRequest::Enabled => app.toggle_settings_network_enabled(cx),
                ProxyRequest::RemoteDns => app.toggle_settings_network_remote_dns(cx),
                ProxyRequest::Test => app.start_settings_network_proxy_test(cx),
                ProxyRequest::SavePassword => app.save_settings_network_proxy_password(cx),
                ProxyRequest::RemovePassword => app.remove_settings_network_proxy_password(cx),
            });
        });
        self.with_settings_controls_for(owner, cx, |controls, cx| {
            let page = ProxyPage {
                controls,
                state: &self.settings_workspace,
                dispatch,
            };
            match section {
                SettingsSectionId::NetworkSharedProxy => {
                    page.shared_proxy(page.settings.network.upstream_proxy.as_ref(), cx)
                }
                SettingsSectionId::NetworkRouting => page.routing(cx),
                _ => div().into_any_element(),
            }
        })
    }

    pub(in crate::workspace) fn toggle_settings_network_disclaimer(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                settings.network.upstream_proxy_disclaimer_accepted =
                    !settings.network.upstream_proxy_disclaimer_accepted;
            },
            cx,
        );
    }

    pub(in crate::workspace) fn toggle_settings_network_enabled(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_network_proxy_password_status(None, cx);
        });
        let removes_saved_password = self
            .settings_store
            .settings()
            .network
            .upstream_proxy
            .as_ref()
            .is_some_and(|proxy| {
                matches!(
                    &proxy.auth,
                    SettingsUpstreamProxyAuth::Password {
                        keychain_id: Some(_),
                        ..
                    }
                )
            });
        if removes_saved_password
            && let Err(error) = self
                .connection_store
                .delete_global_upstream_proxy_password()
        {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.set_network_proxy_password_status(Some(error.to_string()), cx);
            });
            return;
        }
        // Disabling the proxy also clears any transient credential draft.
        self.settings_workspace.update(cx, |settings, cx| {
            settings.finish_network_proxy_password_action(None, cx);
        });
        self.edit_settings(
            |settings| {
                if settings.network.upstream_proxy.is_some() {
                    settings.network.application_proxy_mode = SettingsApplicationProxyMode::System;
                }
                settings.network.upstream_proxy = if settings.network.upstream_proxy.is_some() {
                    None
                } else {
                    Some(default_settings_upstream_proxy_config())
                };
            },
            cx,
        );
    }

    pub(in crate::workspace) fn toggle_settings_network_remote_dns(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                    proxy.remote_dns = !proxy.remote_dns;
                }
            },
            cx,
        );
    }

    pub(in crate::workspace) fn save_settings_network_proxy_password(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(secret) = self
            .settings_workspace
            .update(cx, |settings, _cx| settings.take_network_proxy_password())
        else {
            return;
        };
        match self
            .connection_store
            .save_global_upstream_proxy_password(&secret)
        {
            Ok(keychain_id) => {
                self.edit_settings(
                    move |settings| {
                        if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                            if let SettingsUpstreamProxyAuth::Password { username, .. } =
                                &proxy.auth
                            {
                                proxy.auth = SettingsUpstreamProxyAuth::Password {
                                    username: username.clone(),
                                    keychain_id: Some(keychain_id),
                                };
                            }
                        }
                    },
                    cx,
                );
                let status = self
                    .i18n
                    .t("settings_view.network.password_saved_placeholder");
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_network_proxy_password_action(Some(status), cx);
                });
            }
            Err(error) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.restore_network_proxy_password(secret, error.to_string(), cx);
                });
            }
        }
    }

    pub(in crate::workspace) fn remove_settings_network_proxy_password(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        match self
            .connection_store
            .delete_global_upstream_proxy_password()
        {
            Ok(()) => {
                self.edit_settings(
                    |settings| {
                        if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                            if let SettingsUpstreamProxyAuth::Password { username, .. } =
                                &proxy.auth
                            {
                                proxy.auth = SettingsUpstreamProxyAuth::Password {
                                    username: username.clone(),
                                    keychain_id: None,
                                };
                            }
                        }
                    },
                    cx,
                );
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_network_proxy_password_action(None, cx);
                });
            }
            Err(error) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.set_network_proxy_password_status(Some(error.to_string()), cx);
                });
            }
        }
    }

    pub(in crate::workspace) fn start_settings_network_proxy_test(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(proxy) = self
            .settings_store
            .settings()
            .network
            .upstream_proxy
            .as_ref()
            .cloned()
        else {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.set_network_proxy_test_error("proxy is disabled".to_string(), cx);
            });
            return;
        };
        let Ok(upstream_proxy) =
            upstream_proxy_config_from_global_settings(&self.connection_store, &proxy)
        else {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.set_network_proxy_test_error(
                    "proxy password is not available".to_string(),
                    cx,
                );
            });
            return;
        };
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_network_proxy_test(
                runtime,
                upstream_proxy,
                schedule_settings_network_proxy_test,
                cx,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{AppContext, TestAppContext};
    use oxideterm_ssh::{UpstreamProxyAuth, UpstreamProxyProtocol};

    #[test]
    fn stdio_mcp_json_escapes_the_one_time_credential() {
        let credential = "credential-with-\"quote\\slash";
        let config = public_mcp_stdio_json(credential);
        let value: serde_json::Value =
            serde_json::from_str(&config).expect("parse generated stdio MCP configuration");

        assert_eq!(value["command"], CLI_COMPANION_COMMAND_NAME);
        assert_eq!(value["args"], serde_json::json!(["mcp", "bridge"]));
        assert_eq!(value["env"]["OXIDETERM_MCP_TOKEN"], credential);
    }

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
