use super::*;
use oxideterm_gpui_settings_view::mcp_page::McpSettingsRequest;
use oxideterm_gpui_settings_view::proxy_page::ProxyRequest;

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

impl WorkspaceApp {
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

    pub(in crate::workspace) fn render_public_mcp_client_editor_modal(
        &self,
        cx: &App,
    ) -> Option<AnyElement> {
        self.settings_workspace
            .read(cx)
            .public_mcp_client_editor
            .is_some()
            .then(|| {
                self.settings_views
                    .mcp_client_dialog
                    .clone()
                    .into_any_element()
            })
    }

    pub(super) fn handle_mcp_settings_request(
        &mut self,
        request: McpSettingsRequest,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn handle_proxy_request(&mut self, request: ProxyRequest, cx: &mut Context<Self>) {
        let persisted = self.settings_store.settings();
        let edit = self.settings_workspace.update(cx, |settings, cx| {
            settings.apply_proxy_request(
                request,
                persisted,
                &mut self.connection_store,
                &self.i18n,
                self.forwarding_runtime.clone(),
                |store, config| upstream_proxy_config_from_global_settings(store, config).ok(),
                cx,
            )
        });
        if let Some(edit) = edit {
            self.edit_settings(edit, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
