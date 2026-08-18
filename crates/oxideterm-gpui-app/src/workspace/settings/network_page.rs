use super::*;

pub(in crate::workspace) const SETTINGS_NETWORK_FIELD_WIDTH: f32 = 320.0; // Desktop preference for normal proxy fields.
pub(in crate::workspace) const SETTINGS_NETWORK_PORT_FIELD_WIDTH: f32 = 140.0; // Ports should stay compact instead of sharing a full row.
// External MCP rows use compact settings-card spacing rather than terminal layout metrics.
const SETTINGS_PUBLIC_MCP_ENDPOINT_GAP: f32 = 12.0;
const SETTINGS_PUBLIC_MCP_ROW_GAP: f32 = 10.0;
const SETTINGS_PUBLIC_MCP_STATUS_GAP: f32 = 2.0;
const SETTINGS_PUBLIC_MCP_DETAIL_GAP: f32 = 4.0;
const SETTINGS_PUBLIC_MCP_CREDENTIAL_PADDING: f32 = 12.0;
const SETTINGS_PUBLIC_MCP_COMMAND_PADDING_X: f32 = 8.0;
const SETTINGS_PUBLIC_MCP_COMMAND_PADDING_Y: f32 = 6.0;
const SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE: f32 = 14.0;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NetworkProxyAuthMode {
    None,
    Password,
}

pub(in crate::workspace) fn default_settings_upstream_proxy_config() -> SettingsUpstreamProxyConfig
{
    SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "127.0.0.1".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    }
}

pub(in crate::workspace) fn network_proxy_protocol_label(
    protocol: SettingsUpstreamProxyProtocol,
    i18n: &I18n,
) -> String {
    match protocol {
        SettingsUpstreamProxyProtocol::Socks5 => i18n.t("settings_view.network.protocol_socks5"),
        SettingsUpstreamProxyProtocol::HttpConnect => {
            i18n.t("settings_view.network.protocol_http_connect")
        }
    }
}

pub(in crate::workspace) fn network_proxy_auth_label(
    mode: NetworkProxyAuthMode,
    i18n: &I18n,
) -> String {
    match mode {
        NetworkProxyAuthMode::None => i18n.t("settings_view.network.auth_none"),
        NetworkProxyAuthMode::Password => i18n.t("settings_view.network.auth_password"),
    }
}

pub(in crate::workspace) fn network_application_proxy_mode_label(
    mode: SettingsApplicationProxyMode,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    match mode {
        SettingsApplicationProxyMode::System => {
            i18n.t("settings_view.network.application_mode_system")
        }
        SettingsApplicationProxyMode::Direct => {
            i18n.t("settings_view.network.application_mode_direct")
        }
        SettingsApplicationProxyMode::Shared => {
            i18n.t("settings_view.network.application_mode_shared")
        }
    }
}

fn public_mcp_tool_group_label_key(tool_group: oxideterm_public_mcp::ToolGroup) -> &'static str {
    match tool_group {
        oxideterm_public_mcp::ToolGroup::Basic => "settings_view.network.mcp_group_basic",
        oxideterm_public_mcp::ToolGroup::ConnectionDirectory => {
            "settings_view.network.mcp_group_connection_directory"
        }
        oxideterm_public_mcp::ToolGroup::ConnectionRead => {
            "settings_view.network.mcp_group_connection_read"
        }
        oxideterm_public_mcp::ToolGroup::ConnectionManage => {
            "settings_view.network.mcp_group_connection_manage"
        }
        oxideterm_public_mcp::ToolGroup::CredentialManage => {
            "settings_view.network.mcp_group_credential_manage"
        }
        oxideterm_public_mcp::ToolGroup::NodeSession => {
            "settings_view.network.mcp_group_node_session"
        }
        oxideterm_public_mcp::ToolGroup::TerminalSession => {
            "settings_view.network.mcp_group_terminal_session"
        }
        oxideterm_public_mcp::ToolGroup::TerminalObserve => {
            "settings_view.network.mcp_group_terminal_observe"
        }
        oxideterm_public_mcp::ToolGroup::TerminalInput => {
            "settings_view.network.mcp_group_terminal_input"
        }
        oxideterm_public_mcp::ToolGroup::RecordingControl => {
            "settings_view.network.mcp_group_recording_control"
        }
        oxideterm_public_mcp::ToolGroup::RecordingContent => {
            "settings_view.network.mcp_group_recording_content"
        }
        oxideterm_public_mcp::ToolGroup::DesktopSession => {
            "settings_view.network.mcp_group_desktop_session"
        }
        oxideterm_public_mcp::ToolGroup::DesktopObserve => {
            "settings_view.network.mcp_group_desktop_observe"
        }
        oxideterm_public_mcp::ToolGroup::DesktopInput => {
            "settings_view.network.mcp_group_desktop_input"
        }
        oxideterm_public_mcp::ToolGroup::DesktopClipboard => {
            "settings_view.network.mcp_group_desktop_clipboard"
        }
        oxideterm_public_mcp::ToolGroup::CommandObserve => {
            "settings_view.network.mcp_group_command_observe"
        }
        oxideterm_public_mcp::ToolGroup::CommandExecute => {
            "settings_view.network.mcp_group_command_execute"
        }
        oxideterm_public_mcp::ToolGroup::AuditRead => "settings_view.network.mcp_group_audit_read",
        oxideterm_public_mcp::ToolGroup::ArtifactTransfer => {
            "settings_view.network.mcp_group_artifact_transfer"
        }
        oxideterm_public_mcp::ToolGroup::HostToolsObserve => {
            "settings_view.network.mcp_group_host_tools_observe"
        }
        oxideterm_public_mcp::ToolGroup::HostToolsOperate => {
            "settings_view.network.mcp_group_host_tools_operate"
        }
        oxideterm_public_mcp::ToolGroup::QuickCommandRead => {
            "settings_view.network.mcp_group_quick_command_read"
        }
        oxideterm_public_mcp::ToolGroup::QuickCommandContentRead => {
            "settings_view.network.mcp_group_quick_command_content_read"
        }
        oxideterm_public_mcp::ToolGroup::QuickCommandManage => {
            "settings_view.network.mcp_group_quick_command_manage"
        }
        oxideterm_public_mcp::ToolGroup::QuickCommandExecute => {
            "settings_view.network.mcp_group_quick_command_execute"
        }
        oxideterm_public_mcp::ToolGroup::AddonRead => "settings_view.network.mcp_group_addon_read",
        oxideterm_public_mcp::ToolGroup::AddonManage => {
            "settings_view.network.mcp_group_addon_manage"
        }
        oxideterm_public_mcp::ToolGroup::ForwardRead => {
            "settings_view.network.mcp_group_forward_read"
        }
        oxideterm_public_mcp::ToolGroup::ForwardManage => {
            "settings_view.network.mcp_group_forward_manage"
        }
        oxideterm_public_mcp::ToolGroup::FileRead => "settings_view.network.mcp_group_file_read",
        oxideterm_public_mcp::ToolGroup::FileWrite => "settings_view.network.mcp_group_file_write",
        oxideterm_public_mcp::ToolGroup::WorkspaceRead => {
            "settings_view.network.mcp_group_workspace_read"
        }
        oxideterm_public_mcp::ToolGroup::WorkspaceEdit => {
            "settings_view.network.mcp_group_workspace_edit"
        }
        oxideterm_public_mcp::ToolGroup::CloudSync => "settings_view.network.mcp_group_cloud_sync",
    }
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

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn network_proxy_layout_flags(&self) -> (bool, bool, bool) {
        (
            self.network_proxy_password_status.is_some(),
            self.network_proxy_test_pending,
            self.network_proxy_test_result.is_some(),
        )
    }

    pub(in crate::workspace) fn network_proxy_password_snapshot(
        &self,
    ) -> NetworkProxyPasswordSnapshot {
        NetworkProxyPasswordSnapshot {
            // The input control borrows plaintext directly from the Entity.
            password_present: !self.network_proxy_password.is_empty(),
            password_status: self.network_proxy_password_status.clone(),
        }
    }

    pub(in crate::workspace) fn network_proxy_test_snapshot(&self) -> NetworkProxyTestSnapshot {
        NetworkProxyTestSnapshot {
            test_host: self.network_proxy_test_host.clone(),
            test_port: self.network_proxy_test_port.clone(),
            test_pending: self.network_proxy_test_pending,
            test_result: self.network_proxy_test_result.clone(),
        }
    }

    pub(in crate::workspace) fn take_network_proxy_password(&mut self) -> Option<SecretString> {
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

    pub(in crate::workspace) fn restore_network_proxy_password(
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

    pub(in crate::workspace) fn finish_network_proxy_password_action(
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

    pub(in crate::workspace) fn set_network_proxy_password_status(
        &mut self,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.network_proxy_password_status = status;
        cx.notify();
    }

    pub(in crate::workspace) fn set_network_proxy_test_error(
        &mut self,
        error: String,
        cx: &mut Context<Self>,
    ) {
        self.network_proxy_test_result = Some(Err(error));
        cx.notify();
    }

    pub(in crate::workspace) fn start_network_proxy_test(
        &mut self,
        runtime: std::sync::Arc<tokio::runtime::Runtime>,
        upstream_proxy: UpstreamProxyConfig,
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
        let started_at = std::time::Instant::now();
        let worker =
            schedule_settings_network_proxy_test(runtime.as_ref(), host, port, upstream_proxy);
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

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_network_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let proxy = settings.network.upstream_proxy.as_ref();
        match section_index {
            0 => self.settings_network_shared_proxy_section(proxy, cx),
            1 => self.settings_network_routing_section(cx),
            2 => self.settings_public_mcp_section(cx),
            _ => div().into_any_element(),
        }
    }

    fn settings_public_mcp_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let endpoint = self.public_mcp.endpoint_url().map(str::to_owned);
        let startup_error = self.public_mcp.startup_error().map(str::to_owned);
        let clients = self.public_mcp.clients();
        let client_labels = clients
            .iter()
            .map(|client| (client.client_ref.clone(), client.label.clone()))
            .collect::<HashMap<_, _>>();
        let approvals = self.public_mcp.approvals();
        let credential_ready = self.public_mcp.revealed_credential().is_some();

        let mut content = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap));

        if let Some(error) = startup_error {
            content = content.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(error),
            );
        }
        if let Some(endpoint) = endpoint {
            let endpoint_for_copy = endpoint.clone();
            content = content.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(SETTINGS_PUBLIC_MCP_ENDPOINT_GAP))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(endpoint),
                    )
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.copy_endpoint"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Copy,
                            SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                            rgb(self.tokens.ui.text),
                        )),
                        ToolbarButtonOptions::default(),
                        cx.listener(move |_this, _event, _window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                endpoint_for_copy.clone(),
                            ));
                            cx.stop_propagation();
                        }),
                    )),
            );
        }

        content = content.child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_wrap()
                .items_end()
                .gap(px(SETTINGS_PUBLIC_MCP_ENDPOINT_GAP))
                .child(
                    div()
                        .w(px(SETTINGS_NETWORK_PORT_FIELD_WIDTH))
                        .max_w_full()
                        .child(self.network_input_field(
                            "settings_view.network.public_mcp_port",
                            "settings_view.network.public_mcp_port_hint",
                            SettingsInput::PublicMcpPort,
                            self.current_settings_input_value(SettingsInput::PublicMcpPort, cx),
                            "0".to_string(),
                            true,
                            cx,
                        )),
                )
                .child(self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.network.apply_public_mcp_port"),
                    None,
                    ToolbarButtonOptions::default(),
                    cx.listener(move |this, _event, _window, cx| {
                        let draft = this.public_mcp.port_draft().trim().to_owned();
                        let Ok(port) = draft.parse::<u16>() else {
                            let error =
                                this.i18n.t("settings_view.network.invalid_public_mcp_port");
                            this.public_mcp.record_action_error(error);
                            cx.notify();
                            return;
                        };
                        let runtime = this.forwarding_runtime.handle().clone();
                        if let Err(error) = this.public_mcp.apply_preferred_port(&runtime, port) {
                            this.public_mcp.record_action_error(error.to_string());
                        }
                        cx.notify();
                    }),
                )),
        );

        content = content.child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(
                    self.i18n
                        .t("settings_view.network.public_mcp_security_hint"),
                ),
        );

        if credential_ready {
            content = content.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.warning))
                    .p(px(SETTINGS_PUBLIC_MCP_CREDENTIAL_PADDING))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(SETTINGS_PUBLIC_MCP_ROW_GAP))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(SETTINGS_PUBLIC_MCP_STATUS_GAP))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.network.credential_once")),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.warning))
                                    .child(self.i18n.t("settings_view.network.stdio_json_hint")),
                            ),
                    )
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.copy_credential"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Copy,
                            SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                            rgb(self.tokens.ui.bg),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Default,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(credential) = this.public_mcp.revealed_credential() {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    credential.to_owned(),
                                ));
                            }
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.copy_stdio_json"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Copy,
                            SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                            rgb(self.tokens.ui.text),
                        )),
                        ToolbarButtonOptions::default(),
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(credential) = this.public_mcp.revealed_credential() {
                                let mut config = public_mcp_stdio_json(credential);
                                // The clipboard is the explicit external boundary; app state never retains the JSON.
                                let clipboard_config = std::mem::take(&mut *config);
                                cx.write_to_clipboard(ClipboardItem::new_string(clipboard_config));
                            }
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.dismiss_credential"),
                        None,
                        ToolbarButtonOptions::default(),
                        cx.listener(|this, _event, _window, cx| {
                            this.public_mcp.dismiss_revealed_credential();
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )),
            );
        }

        content = content
            .child(self.card_separator())
            .child(self.network_subsection_heading(
                "settings_view.network.external_clients",
                "settings_view.network.external_clients_hint",
            ));

        if clients.is_empty() {
            content = content.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.network.no_external_clients")),
            );
        } else {
            for client in clients {
                let client_ref_for_toggle = client.client_ref.clone();
                let client_ref_for_remove = client.client_ref.clone();
                let client_ref_for_mode = client.client_ref.clone();
                let next_enabled = !client.enabled;
                let next_approval_mode = match client.approval_mode {
                    oxideterm_public_mcp::ClientApprovalMode::Standard => {
                        oxideterm_public_mcp::ClientApprovalMode::Unattended
                    }
                    oxideterm_public_mcp::ClientApprovalMode::Unattended => {
                        oxideterm_public_mcp::ClientApprovalMode::Standard
                    }
                };
                let status_key = if client.enabled {
                    "settings_view.network.client_enabled"
                } else {
                    "settings_view.network.client_disabled"
                };
                let toggle_key = if client.enabled {
                    "settings_view.network.disable_client"
                } else {
                    "settings_view.network.enable_client"
                };
                let mode_key = match client.approval_mode {
                    oxideterm_public_mcp::ClientApprovalMode::Standard => {
                        "settings_view.network.client_mode_standard"
                    }
                    oxideterm_public_mcp::ClientApprovalMode::Unattended => {
                        "settings_view.network.client_mode_full"
                    }
                };
                let mode_toggle_key = match client.approval_mode {
                    oxideterm_public_mcp::ClientApprovalMode::Standard => {
                        "settings_view.network.switch_to_full_mode"
                    }
                    oxideterm_public_mcp::ClientApprovalMode::Unattended => {
                        "settings_view.network.switch_to_standard_mode"
                    }
                };
                let mut group_controls = div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(SETTINGS_PUBLIC_MCP_ROW_GAP));
                let visible_tool_groups = std::iter::once(oxideterm_public_mcp::ToolGroup::Basic)
                    .chain(
                        oxideterm_public_mcp::ToolGroup::selectable()
                            .iter()
                            .copied(),
                    );
                for tool_group in visible_tool_groups {
                    let label_key = public_mcp_tool_group_label_key(tool_group);
                    let checked = client.tool_groups.contains(&tool_group);
                    let client_ref_for_group = client.client_ref.clone();
                    let required_group = tool_group == oxideterm_public_mcp::ToolGroup::Basic;
                    let mut control = checkbox_with_state(
                        &self.tokens,
                        String::new(),
                        if checked {
                            CheckboxState::Checked
                        } else {
                            CheckboxState::Unchecked
                        },
                        CheckboxOptions {
                            disabled: required_group,
                            ..CheckboxOptions::default()
                        },
                    );
                    if !required_group {
                        control = control.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if let Err(error) = this.set_public_mcp_client_tool_group(
                                    &client_ref_for_group,
                                    tool_group,
                                    !checked,
                                    cx,
                                ) {
                                    this.public_mcp.record_action_error(error);
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        );
                    }
                    group_controls = group_controls.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(SETTINGS_PUBLIC_MCP_STATUS_GAP))
                            .child(control)
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t(label_key)),
                            ),
                    );
                }
                content = content.child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(SETTINGS_PUBLIC_MCP_DETAIL_GAP))
                        .child(
                            div()
                                .w_full()
                                .min_w(px(0.0))
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(SETTINGS_PUBLIC_MCP_ROW_GAP))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .gap(px(SETTINGS_PUBLIC_MCP_STATUS_GAP))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(client.label),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(format!(
                                                    "{} · {}",
                                                    self.i18n.t(status_key),
                                                    self.i18n.t(mode_key)
                                                )),
                                        ),
                                )
                                .child(self.workspace_toolbar_action_button(
                                    self.i18n.t(mode_toggle_key),
                                    None,
                                    ToolbarButtonOptions::default(),
                                    cx.listener(move |this, _event, _window, cx| {
                                        if let Err(error) = this
                                            .set_public_mcp_client_approval_mode(
                                                &client_ref_for_mode,
                                                next_approval_mode,
                                                cx,
                                            )
                                        {
                                            this.public_mcp.record_action_error(error);
                                        }
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                ))
                                .child(self.workspace_toolbar_action_button(
                                    self.i18n.t(toggle_key),
                                    None,
                                    ToolbarButtonOptions::default(),
                                    cx.listener(move |this, _event, _window, cx| {
                                        if let Err(error) = this.set_public_mcp_client_enabled(
                                            &client_ref_for_toggle,
                                            next_enabled,
                                            cx,
                                        ) {
                                            this.public_mcp.record_action_error(error);
                                        }
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                ))
                                .child(self.workspace_toolbar_action_button(
                                    self.i18n.t("settings_view.network.revoke_client"),
                                    Some(Self::render_lucide_icon(
                                        LucideIcon::Trash2,
                                        SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                                        rgb(self.tokens.ui.text),
                                    )),
                                    ToolbarButtonOptions::default(),
                                    cx.listener(move |this, _event, _window, cx| {
                                        if let Err(error) = this
                                            .remove_public_mcp_client(&client_ref_for_remove, cx)
                                        {
                                            this.public_mcp.record_action_error(error);
                                        }
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                )),
                        )
                        .child(group_controls),
                );
            }
        }

        let next_client_number = self.public_mcp.clients().len() + 1;
        content = content.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(SETTINGS_PUBLIC_MCP_ROW_GAP))
                .child(self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.network.create_standard_client"),
                    Some(Self::render_lucide_icon(
                        LucideIcon::Plus,
                        SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.bg),
                    )),
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Default,
                            ..ButtonOptions::default()
                        },
                        ..ToolbarButtonOptions::default()
                    },
                    cx.listener(move |this, _event, _window, cx| {
                        let label = format!(
                            "{} {}",
                            this.i18n.t("settings_view.network.external_client_name"),
                            next_client_number
                        );
                        if let Err(error) = this.public_mcp.create_client(
                            label,
                            oxideterm_public_mcp::ClientApprovalMode::Standard,
                        ) {
                            this.public_mcp.record_action_error(error);
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ))
                .child(self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.network.create_full_client"),
                    None,
                    ToolbarButtonOptions::default(),
                    cx.listener(move |this, _event, _window, cx| {
                        let label = format!(
                            "{} {}",
                            this.i18n.t("settings_view.network.external_client_name"),
                            next_client_number
                        );
                        if let Err(error) = this.public_mcp.create_client(
                            label,
                            oxideterm_public_mcp::ClientApprovalMode::Unattended,
                        ) {
                            this.public_mcp.record_action_error(error);
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )),
        );

        content = content
            .child(self.card_separator())
            .child(self.network_subsection_heading(
                "settings_view.network.pending_approvals",
                "settings_view.network.pending_approvals_hint",
            ));
        let pending = approvals
            .into_iter()
            .filter(|approval| approval.status == oxideterm_public_mcp::ApprovalStatus::Pending)
            .collect::<Vec<_>>();
        if pending.is_empty() {
            content = content.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.network.no_pending_approvals")),
            );
        } else {
            for approval in pending {
                let approval_ref_for_accept = approval.approval_ref.clone();
                let approval_ref_for_reject = approval.approval_ref.clone();
                let client_label = client_labels
                    .get(&approval.client_ref)
                    .cloned()
                    .unwrap_or_else(|| approval.client_ref.to_string());
                let client_summary = self
                    .i18n
                    .t("settings_view.network.approval_client")
                    .replace("{{client}}", &client_label);
                let target_label =
                    self.public_mcp_target_label(&approval.client_ref, &approval.target, cx);
                let mut approval_details = div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(SETTINGS_PUBLIC_MCP_DETAIL_GAP))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(approval.tool_name),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(client_summary),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(target_label),
                    );
                if let Some(command) = approval.review.command() {
                    let command_summary = self
                        .i18n
                        .t("settings_view.network.approval_command")
                        .replace("{{command}}", command);
                    approval_details = approval_details.child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .rounded(px(self.tokens.radii.sm))
                            .bg(rgb(self.tokens.ui.bg_hover))
                            .px(px(SETTINGS_PUBLIC_MCP_COMMAND_PADDING_X))
                            .py(px(SETTINGS_PUBLIC_MCP_COMMAND_PADDING_Y))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text))
                            .whitespace_normal()
                            .child(command_summary),
                    );
                }
                if let Some(working_directory) = approval.review.working_directory() {
                    approval_details = approval_details.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.network.approval_working_directory")
                                    .replace("{{directory}}", working_directory),
                            ),
                    );
                }
                content = content.child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(SETTINGS_PUBLIC_MCP_ROW_GAP))
                        .child(approval_details)
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.approve_action"),
                            Some(Self::render_lucide_icon(
                                LucideIcon::Check,
                                SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                                rgb(self.tokens.ui.bg),
                            )),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    ..ButtonOptions::default()
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                if let Err(error) = this.public_mcp.set_approval_status(
                                    &approval_ref_for_accept,
                                    oxideterm_public_mcp::ApprovalStatus::Approved,
                                ) {
                                    this.public_mcp.record_action_error(error);
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ))
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.reject_action"),
                            None,
                            ToolbarButtonOptions::default(),
                            cx.listener(move |this, _event, _window, cx| {
                                if let Err(error) = this.public_mcp.set_approval_status(
                                    &approval_ref_for_reject,
                                    oxideterm_public_mcp::ApprovalStatus::Rejected,
                                ) {
                                    this.public_mcp.record_action_error(error);
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        )),
                );
            }
        }

        self.settings_card(
            "settings_view.network.public_mcp",
            "settings_view.network.public_mcp_hint",
            vec![content.into_any_element()],
        )
    }

    fn settings_network_shared_proxy_section(
        &self,
        proxy: Option<&SettingsUpstreamProxyConfig>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(proxy) = proxy else {
            let disclaimer_accepted = self
                .settings_store
                .settings()
                .network
                .upstream_proxy_disclaimer_accepted;
            let empty_state = div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_card_gap))
                .child(self.network_checkbox_row(
                    "settings_view.network.disclaimer",
                    "settings_view.network.disclaimer_hint",
                    disclaimer_accepted,
                    true,
                    Self::toggle_settings_network_disclaimer,
                    cx,
                ))
                .child(
                    div()
                        .flex_none()
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.add_shared_proxy"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Default,
                                    radius: ButtonRadius::Md,
                                    disabled: !disclaimer_accepted,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.toggle_settings_network_enabled(cx);
                                cx.stop_propagation();
                            }),
                        )),
                )
                .into_any_element();
            return self.settings_card(
                "settings_view.network.shared_proxy",
                "settings_view.network.shared_proxy_empty_hint",
                vec![empty_state],
            );
        };

        let content =
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_card_gap))
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .gap(px(32.0))
                        .child(self.network_responsive_field(
                            SETTINGS_NETWORK_FIELD_WIDTH,
                            self.network_select_field(
                                "settings_view.network.protocol",
                                "settings_view.network.protocol_hint",
                                SettingsSelect::NetworkProxyProtocol,
                                network_proxy_protocol_label(proxy.protocol, &self.i18n),
                                true,
                                cx,
                            ),
                        ))
                        .child(self.network_compact_field(
                            SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                            self.network_input_field(
                                "settings_view.network.port",
                                "settings_view.network.port_hint",
                                SettingsInput::NetworkProxyPort,
                                proxy.port.to_string(),
                                "1080".to_string(),
                                true,
                                cx,
                            ),
                        )),
                )
                .child(self.network_full_width_input(
                    "settings_view.network.host",
                    "settings_view.network.host_hint",
                    SettingsInput::NetworkProxyHost,
                    proxy.host.clone(),
                    "127.0.0.1".to_string(),
                    true,
                    cx,
                ))
                .child(self.network_full_width_input(
                    "settings_view.network.no_proxy",
                    "settings_view.network.no_proxy_hint",
                    SettingsInput::NetworkProxyNoProxy,
                    proxy.no_proxy.clone(),
                    "localhost,127.0.0.1,*.internal".to_string(),
                    true,
                    cx,
                ))
                .child(self.network_checkbox_row(
                    "settings_view.network.remote_dns",
                    "settings_view.network.remote_dns_hint",
                    proxy.remote_dns,
                    true,
                    Self::toggle_settings_network_remote_dns,
                    cx,
                ))
                .child(self.card_separator())
                .child(self.settings_network_auth_content(Some(proxy), cx))
                .child(self.card_separator())
                .child(self.settings_network_test_content(true, cx))
                .child(self.card_separator())
                .child(div().w_full().flex().justify_end().child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.remove_shared_proxy"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Trash2,
                            16.0,
                            rgb(self.tokens.ui.error),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_settings_network_enabled(cx);
                            cx.stop_propagation();
                        }),
                    ),
                ))
                .into_any_element();

        self.settings_card(
            "settings_view.network.shared_proxy",
            "settings_view.network.shared_proxy_hint",
            vec![content],
        )
    }

    fn settings_network_auth_content(
        &self,
        proxy: Option<&SettingsUpstreamProxyConfig>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auth_mode = proxy
            .map(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                SettingsUpstreamProxyAuth::Password { .. } => NetworkProxyAuthMode::Password,
            })
            .unwrap_or(NetworkProxyAuthMode::None);
        let auth_username = proxy
            .and_then(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::Password { username, .. } => Some(username.clone()),
                SettingsUpstreamProxyAuth::None => None,
            })
            .unwrap_or_default();
        let auth_has_saved_password = proxy.is_some_and(|proxy| match &proxy.auth {
            SettingsUpstreamProxyAuth::Password { keychain_id, .. } => keychain_id.is_some(),
            SettingsUpstreamProxyAuth::None => false,
        });

        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .opacity(if proxy.is_some() { 1.0 } else { 0.4 })
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(32.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_select_field(
                            "settings_view.network.auth",
                            "settings_view.network.auth_hint",
                            SettingsSelect::NetworkProxyAuth,
                            network_proxy_auth_label(auth_mode, &self.i18n),
                            proxy.is_some(),
                            cx,
                        ),
                    )),
            );

        if auth_mode == NetworkProxyAuthMode::Password {
            section = section
                .child(self.network_full_width_input(
                    "settings_view.network.username",
                    "settings_view.network.username_hint",
                    SettingsInput::NetworkProxyUsername,
                    auth_username,
                    String::new(),
                    proxy.is_some(),
                    cx,
                ))
                .child(self.network_password_field(auth_has_saved_password, proxy.is_some(), cx));
        }

        section.into_any_element()
    }

    fn settings_network_test_content(
        &self,
        proxy_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshot = self
            .settings_workspace
            .read(cx)
            .network_proxy_test_snapshot();
        let host_value = snapshot.test_host;
        let port_value = snapshot.test_port;
        let test_disabled = !proxy_enabled
            || snapshot.test_pending
            || host_value.trim().is_empty()
            || port_value.trim().parse::<u16>().is_err();
        let status = snapshot.test_result.map(|result| match result {
            Ok(elapsed) => self
                .i18n
                .t("settings_view.network.test_success")
                .replace("{{elapsed}}", &elapsed.to_string()),
            Err(error) => self
                .i18n
                .t("settings_view.network.test_error")
                .replace("{{error}}", &error),
        });

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .opacity(if proxy_enabled { 1.0 } else { 0.4 })
            .child(self.network_subsection_heading(
                "settings_view.network.test_title",
                "settings_view.network.test_hint",
            ))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(16.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_host",
                            "settings_view.network.test_host_hint",
                            SettingsInput::NetworkProxyTestHost,
                            host_value,
                            "server.example.com".to_string(),
                            proxy_enabled,
                            cx,
                        ),
                    ))
                    .child(self.network_compact_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_port",
                            "settings_view.network.test_port_hint",
                            SettingsInput::NetworkProxyTestPort,
                            port_value,
                            "22".to_string(),
                            proxy_enabled,
                            cx,
                        ),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                if snapshot.test_pending {
                                    self.i18n.t("settings_view.network.testing")
                                } else {
                                    self.i18n.t("settings_view.network.test_button")
                                },
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: test_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_settings_network_proxy_test(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    )
                    .when_some(status, |row, status| {
                        row.child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(status),
                        )
                    }),
            )
            .into_any_element()
    }

    fn settings_network_routing_section(&self, cx: &mut Context<Self>) -> AnyElement {
        // Routing policy is distinct from the reusable proxy definition above.
        // SSH remains connection-owned while app and updater routes are global.
        let settings = self.settings_store.settings();
        let application_mode = settings.network.application_proxy_mode;
        let update_proxy = &settings.general.update_proxy;
        let mut content = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.network_static_route_row(
                "settings_view.network.ssh_route",
                "settings_view.network.ssh_route_hint",
                "settings_view.network.per_connection",
            ))
            .child(self.card_separator())
            .child(self.network_route_select_row(
                "settings_view.network.application_route",
                "settings_view.network.application_route_hint",
                SettingsSelect::NetworkApplicationProxyMode,
                network_application_proxy_mode_label(application_mode, &self.i18n),
                cx,
            ))
            .child(self.card_separator())
            .child(self.network_route_select_row(
                "settings_view.network.update_route",
                "settings_view.network.update_route_hint",
                SettingsSelect::UpdateProxyMode,
                update_proxy_mode_label(update_proxy.mode, &self.i18n),
                cx,
            ));

        if update_proxy.mode == UpdateProxyMode::Custom {
            content = content.child(self.settings_network_custom_update_proxy(cx));
        }

        content = content.child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.network.legal_hint")),
        );

        self.settings_card(
            "settings_view.network.routing",
            "settings_view.network.routing_hint",
            vec![content.into_any_element()],
        )
    }

    fn settings_network_custom_update_proxy(&self, cx: &mut Context<Self>) -> AnyElement {
        let proxy = &self.settings_store.settings().general.update_proxy;
        div()
            .w_full()
            .min_w(px(0.0))
            .pt(px(self.tokens.metrics.settings_row_gap))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(self.network_subsection_heading(
                "settings_view.network.custom_update_proxy",
                "settings_view.network.custom_update_proxy_hint",
            ))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(32.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_select_field(
                            "settings_view.network.update_protocol",
                            "settings_view.network.update_protocol_hint",
                            SettingsSelect::UpdateProxyProtocol,
                            update_proxy_protocol_label(proxy.protocol, &self.i18n),
                            true,
                            cx,
                        ),
                    ))
                    .child(self.network_compact_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.update_port",
                            "settings_view.network.update_port_hint",
                            SettingsInput::UpdateProxyPort,
                            self.current_settings_input_value(SettingsInput::UpdateProxyPort, cx),
                            "7890".to_string(),
                            true,
                            cx,
                        ),
                    )),
            )
            .child(self.network_full_width_input(
                "settings_view.network.update_host",
                "settings_view.network.update_host_hint",
                SettingsInput::UpdateProxyHost,
                self.current_settings_input_value(SettingsInput::UpdateProxyHost, cx),
                "127.0.0.1".to_string(),
                true,
                cx,
            ))
            .child(self.network_full_width_input(
                "settings_view.network.update_no_proxy",
                "settings_view.network.update_no_proxy_hint",
                SettingsInput::UpdateProxyNoProxy,
                self.current_settings_input_value(SettingsInput::UpdateProxyNoProxy, cx),
                "localhost,127.0.0.1".to_string(),
                true,
                cx,
            ))
            .into_any_element()
    }

    fn network_subsection_heading(&self, label_key: &str, hint_key: &str) -> AnyElement {
        self.network_field_label(label_key, hint_key)
    }

    fn network_static_route_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value_key: &str,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                    .child(self.network_field_label(label_key, hint_key)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(value_key)),
            )
            .into_any_element()
    }

    fn network_route_select_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                    .child(self.network_field_label(label_key, hint_key)),
            )
            .child(
                div()
                    .w(px(240.0))
                    .max_w_full()
                    .flex_none()
                    .child(self.settings_select_control(select_id, value, false, None, cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn network_responsive_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Field slots grow with the card and wrap once their preferred width
        // no longer fits, avoiding a fixed-width cluster on wide panes.
        div()
            .max_w_full()
            .min_w(px(0.0))
            .flex_1()
            .flex_basis(px(preferred_width))
            .child(field)
            .into_any_element()
    }

    pub(in crate::workspace) fn network_compact_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Compact numeric fields keep their natural width while larger peers
        // consume the remaining row; max-width still permits narrow panes.
        div()
            .w(px(preferred_width))
            .max_w_full()
            .min_w(px(0.0))
            .flex_initial()
            .child(field)
            .into_any_element()
    }

    pub(in crate::workspace) fn network_checkbox_row(
        &self,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        enabled: bool,
        on_toggle: fn(&mut WorkspaceApp, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control =
            checkbox(&self.tokens, String::new(), checked).opacity(if enabled { 1.0 } else { 0.5 });
        if enabled {
            control = control.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    on_toggle(this, cx);
                    cx.stop_propagation();
                }),
            );
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    // Checkbox rows keep the label as the flexible item so the
                    // fixed checkbox can wrap inside narrow settings panes.
                    .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(div().flex_none().child(control.into_any_element()))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.settings_select_control(select_id, value, !enabled, None, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_input_field(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(
                self.settings_text_input_control_fill(input, value, placeholder, cx)
                    .into_any_element(),
            )
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_full_width_input(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.network_full_width_text_input_control(input, value, placeholder, cx))
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_full_width_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .w_full()
            .min_w(px(0.0))
            // Full-width proxy fields must size from their parent column, not
            // from the desktop max width used by other settings controls.
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn network_password_field(
        &self,
        has_saved_password: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let password_input = SettingsInput::NetworkProxyPassword;
        let snapshot = self
            .settings_workspace
            .read(cx)
            .network_proxy_password_snapshot();
        let save_disabled = !snapshot.password_present || !enabled;
        let remove_disabled = !has_saved_password && !snapshot.password_present;
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(
                "settings_view.network.password",
                if has_saved_password {
                    "settings_view.network.password_saved_hint"
                } else {
                    "settings_view.network.password_hint"
                },
            ))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                            .child(self.settings_secret_text_input_control_fill(
                                password_input,
                                String::new(),
                                if has_saved_password {
                                    self.i18n
                                        .t("settings_view.network.password_saved_placeholder")
                                } else {
                                    String::new()
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.network.save_password"),
                                Some(Self::render_lucide_icon(
                                    LucideIcon::KeyRound,
                                    16.0,
                                    rgb(if save_disabled {
                                        self.tokens.ui.text_muted
                                    } else {
                                        self.tokens.ui.bg
                                    }),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: save_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.save_settings_network_proxy_password(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.network.remove_password"),
                                Some(Self::render_lucide_icon(
                                    LucideIcon::Trash2,
                                    16.0,
                                    rgb(self.tokens.ui.text),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: remove_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.remove_settings_network_proxy_password(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            )
            .when(!enabled, |field| field.opacity(0.5));

        if let Some(status) = snapshot.password_status {
            row = row.child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(status),
            );
        }

        row.into_any_element()
    }

    pub(in crate::workspace) fn network_field_label(
        &self,
        label_key: &str,
        hint_key: &str,
    ) -> AnyElement {
        div()
            .min_w(px(0.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
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
            settings.start_network_proxy_test(runtime, upstream_proxy, cx);
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

    #[test]
    fn proxy_route_test_enters_workspace_tokio_runtime() {
        let proxy_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("bind a proxy test listener");
        let proxy_port = proxy_listener
            .local_addr()
            .expect("read proxy address")
            .port();
        let proxy_server = std::thread::spawn(move || {
            // Closing the accepted socket forces a deterministic SOCKS handshake error.
            let (stream, _) = proxy_listener.accept().expect("accept proxy test client");
            drop(stream);
        });

        let runtime = tokio::runtime::Runtime::new().expect("create workspace runtime");
        let worker = schedule_settings_network_proxy_test(
            &runtime,
            "proxy-test-target.invalid".to_string(),
            22,
            UpstreamProxyConfig {
                protocol: UpstreamProxyProtocol::Socks5,
                host: std::net::Ipv4Addr::LOCALHOST.to_string(),
                port: proxy_port,
                auth: UpstreamProxyAuth::None,
                remote_dns: true,
                no_proxy: String::new(),
            },
        );

        let status = runtime
            .block_on(worker)
            .expect("receive the proxy test result outside Tokio");
        proxy_server.join().expect("join proxy test server");
        assert!(matches!(status, HostKeyStatus::Error { .. }));
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
