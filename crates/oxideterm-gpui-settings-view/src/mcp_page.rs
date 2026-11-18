use crate::{
    components::{SettingsCalloutTone, SettingsControls, settings_mono_font_family},
    entity::SettingsWorkspaceEntity,
    surface::nav::render_icon,
    *,
};
use gpui::{
    AnyElement, App, ClipboardItem, Context, Div, Entity, MouseButton, MouseDownEvent, Render,
    ScrollHandle, Subscription, Window, div, prelude::*, px, rgb,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    checkbox::checkbox,
    modal::{
        dialog_description, dialog_footer, dialog_header, dialog_title, dismissible_dialog_backdrop,
    },
    scroll::ScrollableElement,
};
use oxideterm_public_mcp::{
    ApprovalProjection, ApprovalRef, ClientApprovalMode, ClientProjection, ClientRef, ToolGroup,
};
use std::{collections::HashMap, ops::Deref, rc::Rc};

const SETTINGS_NETWORK_PORT_FIELD_WIDTH: f32 = 140.0;
// External MCP rows use compact settings-card spacing rather than terminal layout metrics.
const SETTINGS_PUBLIC_MCP_ENDPOINT_GAP: f32 = 12.0;
const SETTINGS_PUBLIC_MCP_ROW_GAP: f32 = 10.0;
const SETTINGS_PUBLIC_MCP_STATUS_GAP: f32 = 2.0;
const SETTINGS_PUBLIC_MCP_DETAIL_GAP: f32 = 4.0;
const SETTINGS_PUBLIC_MCP_COMMAND_PADDING_X: f32 = 8.0;
const SETTINGS_PUBLIC_MCP_COMMAND_PADDING_Y: f32 = 6.0;
const SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE: f32 = 14.0;
const PUBLIC_MCP_CLIENT_MODAL_WIDTH: f32 = 680.0;
const PUBLIC_MCP_CLIENT_MODAL_MAX_HEIGHT: f32 = 660.0;

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

// These are display families only; each checkbox retains its original ToolGroup authority.
fn public_mcp_permission_sections()
-> [(&'static str, &'static [oxideterm_public_mcp::ToolGroup]); 6] {
    use oxideterm_public_mcp::ToolGroup::*;
    [
        (
            "settings_view.network.mcp_family_connections",
            &[
                ConnectionDirectory,
                ConnectionRead,
                ConnectionManage,
                CredentialManage,
                NodeSession,
            ],
        ),
        (
            "settings_view.network.mcp_family_terminal",
            &[
                TerminalSession,
                TerminalObserve,
                TerminalInput,
                RecordingControl,
                RecordingContent,
                CommandObserve,
                CommandExecute,
                QuickCommandRead,
                QuickCommandContentRead,
                QuickCommandManage,
                QuickCommandExecute,
            ],
        ),
        (
            "settings_view.network.mcp_family_desktop",
            &[
                DesktopSession,
                DesktopObserve,
                DesktopInput,
                DesktopClipboard,
            ],
        ),
        (
            "settings_view.network.mcp_family_files",
            &[
                ArtifactTransfer,
                FileRead,
                FileWrite,
                WorkspaceRead,
                WorkspaceEdit,
            ],
        ),
        (
            "settings_view.network.mcp_family_host",
            &[
                HostToolsObserve,
                HostToolsOperate,
                ForwardRead,
                ForwardManage,
            ],
        ),
        (
            "settings_view.network.mcp_family_other",
            &[AddonRead, AddonManage, AuditRead, CloudSync],
        ),
    ]
}

#[derive(Clone)]
pub enum McpSettingsRequest {
    CreateClient,
    ApplyPort,
    Approve(ApprovalRef),
    Reject(ApprovalRef),
    CopyCredential,
    CopyStdio,
    DismissCredential,
    OpenEditor(String),
    SetToolGroup(ClientRef, ToolGroup, bool),
    SetMode(ClientRef, ClientApprovalMode),
    SetEnabled(ClientRef, bool),
    RemoveClient(ClientRef),
    CloseEditor,
}

pub struct McpSettingsFrame<'a> {
    pub endpoint: Option<&'a str>,
    pub startup_error: Option<&'a str>,
    pub clients: Vec<ClientProjection>,
    pub approvals: Vec<ApprovalProjection>,
    pub port_draft: &'a str,
    pub credential_ready: bool,
}

pub struct McpSettingsPage<'a> {
    pub controls: SettingsControls<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub frame: McpSettingsFrame<'a>,
    pub dispatch: Rc<dyn Fn(McpSettingsRequest, &mut App)>,
    pub target_label: &'a dyn Fn(&ClientRef, &str, &App) -> String,
    pub scroll: &'a ScrollHandle,
}

impl<'a> Deref for McpSettingsPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl McpSettingsPage<'_> {
    fn select_mode(
        &self,
        mode: ClientApprovalMode,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let state = self.state.downgrade();
        move |_, _, cx| {
            let _ = state.update(cx, |state, cx| {
                state.new_mcp_client_mode = mode;
                cx.notify();
            });
            cx.stop_propagation();
        }
    }

    fn press(
        &self,
        request: McpSettingsRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request.clone(), cx);
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }
    fn public_mcp_create_client_controls(&self, cx: &App) -> AnyElement {
        let mode = self.state.read(cx).new_mcp_client_mode;
        let mut picker = div().flex().flex_wrap().gap(px(4.0));
        for (value, label) in [
            (
                oxideterm_public_mcp::ClientApprovalMode::Standard,
                "settings_view.network.client_mode_standard",
            ),
            (
                oxideterm_public_mcp::ClientApprovalMode::Unattended,
                "settings_view.network.client_mode_full",
            ),
        ] {
            picker = picker.child(
                oxideterm_gpui_ui::segmented_control_item(
                    &self.tokens,
                    self.i18n.t(label),
                    mode == value,
                )
                .on_mouse_down(MouseButton::Left, self.select_mode(value)),
            );
        }
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.network.mcp_new_client_mode")),
            )
            .child(picker)
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.network.mcp_create_client"),
                Some(render_icon(
                    "lucide/plus.svg",
                    SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                    rgb(self.tokens.ui.bg),
                )),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Default,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                self.press(McpSettingsRequest::CreateClient),
            ))
            .into_any_element()
    }

    pub fn settings_public_mcp_section(&self, cx: &App) -> AnyElement {
        let endpoint = self.frame.endpoint.map(str::to_owned);
        let startup_error = self.frame.startup_error.map(str::to_owned);
        let clients = &self.frame.clients;
        let client_labels = clients
            .iter()
            .map(|client| (client.client_ref.clone(), client.label.clone()))
            .collect::<HashMap<_, _>>();
        let approvals = &self.frame.approvals;
        let credential_ready = self.frame.credential_ready;

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
                            .font_family(settings_mono_font_family(self.settings))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(endpoint),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.success))
                            .child(self.i18n.t("settings_view.network.mcp_running")),
                    )
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.copy_endpoint"),
                        Some(render_icon(
                            "lucide/copy.svg",
                            SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                            rgb(self.tokens.ui.text),
                        )),
                        ToolbarButtonOptions::default(),
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                endpoint_for_copy.clone(),
                            ));
                            cx.stop_propagation();
                        },
                    )),
            );
        }

        content = content.child(
            self.setting_row(
                "settings_view.network.public_mcp_port",
                "settings_view.network.public_mcp_port_hint",
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_end()
                    .gap(px(SETTINGS_PUBLIC_MCP_ENDPOINT_GAP))
                    .child(
                        div()
                            .w(px(SETTINGS_NETWORK_PORT_FIELD_WIDTH))
                            .max_w_full()
                            .child(self.settings_text_input_control_fill(
                                SettingsInput::PublicMcpPort,
                                self.frame.port_draft,
                                "0".to_string(),
                                cx,
                            )),
                    )
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.apply_public_mcp_port"),
                        None,
                        ToolbarButtonOptions::default(),
                        self.press(McpSettingsRequest::ApplyPort),
                    ))
                    .into_any_element(),
                cx,
            ),
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

        let pending = approvals
            .iter()
            .filter(|approval| approval.status == oxideterm_public_mcp::ApprovalStatus::Pending)
            .collect::<Vec<_>>();
        content = content.child(self.card_separator()).child(
            self.card_subsection_header(
                format!(
                    "{} · {}",
                    self.i18n.t("settings_view.network.pending_approvals"),
                    pending.len()
                ),
                (!pending.is_empty())
                    .then(|| self.i18n.t("settings_view.network.pending_approvals_hint")),
            ),
        );
        if !pending.is_empty() {
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
                let target_label = (self.target_label)(&approval.client_ref, &approval.target, cx);
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
                            .child(approval.tool_name.clone()),
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
                            .font_family(settings_mono_font_family(self.settings))
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
                            Some(render_icon(
                                "lucide/check.svg",
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
                            self.press(McpSettingsRequest::Approve(approval_ref_for_accept)),
                        ))
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.reject_action"),
                            None,
                            ToolbarButtonOptions::default(),
                            self.press(McpSettingsRequest::Reject(approval_ref_for_reject)),
                        )),
                );
            }
        }

        content = content
            .child(self.card_separator())
            .child(self.card_subsection_header(
                self.i18n.t("settings_view.network.external_clients"),
                Some(self.i18n.t("settings_view.network.external_clients_hint")),
            ));

        content = content.child(self.public_mcp_create_client_controls(cx));

        if credential_ready {
            content = content.child(
                self.settings_callout(
                    SettingsCalloutTone::Warning,
                    Some("lucide/alert-triangle.svg"),
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
                                        .child(
                                            self.i18n.t("settings_view.network.credential_once"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.warning))
                                        .child(
                                            self.i18n.t("settings_view.network.stdio_json_hint"),
                                        ),
                                ),
                        )
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.copy_credential"),
                            Some(render_icon(
                                "lucide/copy.svg",
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
                            self.press(McpSettingsRequest::CopyCredential),
                        ))
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.copy_stdio_json"),
                            Some(render_icon(
                                "lucide/copy.svg",
                                SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                                rgb(self.tokens.ui.text),
                            )),
                            ToolbarButtonOptions::default(),
                            self.press(McpSettingsRequest::CopyStdio),
                        ))
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.dismiss_credential"),
                            None,
                            ToolbarButtonOptions::default(),
                            self.press(McpSettingsRequest::DismissCredential),
                        ))
                        .into_any_element(),
                ),
            );
        }

        if clients.is_empty() {
            content = content.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.network.no_external_clients")),
            );
        } else {
            let mut client_rows = Vec::with_capacity(clients.len());
            for client in clients {
                let client_ref_for_edit = client.client_ref.as_str().to_string();
                let status_key = if client.enabled {
                    "settings_view.network.client_enabled"
                } else {
                    "settings_view.network.client_disabled"
                };
                let mode_key = match client.approval_mode {
                    oxideterm_public_mcp::ClientApprovalMode::Standard => {
                        "settings_view.network.client_mode_standard"
                    }
                    oxideterm_public_mcp::ClientApprovalMode::Unattended => {
                        "settings_view.network.client_mode_full"
                    }
                };
                let permission_count = client
                    .tool_groups
                    .iter()
                    .filter(|group| **group != oxideterm_public_mcp::ToolGroup::Basic)
                    .count();
                client_rows.push(
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
                                        .child(client.label.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(format!(
                                            "{} · {} · {}",
                                            self.i18n.t(status_key),
                                            self.i18n.t(mode_key),
                                            self.i18n
                                                .t("settings_view.network.mcp_permission_count")
                                                .replace(
                                                    "{{count}}",
                                                    &permission_count.to_string()
                                                )
                                        )),
                                ),
                        )
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.show_permissions"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                ..Default::default()
                            },
                            self.press(McpSettingsRequest::OpenEditor(client_ref_for_edit)),
                        ))
                        .into_any_element(),
                );
            }
            content = content.children(self.separated_settings_rows(client_rows));
        }

        self.settings_card(
            "settings_view.network.public_mcp",
            "settings_view.network.public_mcp_hint",
            vec![content.into_any_element()],
        )
    }

    pub fn render_public_mcp_client_editor_modal(&self, cx: &App) -> Option<AnyElement> {
        let target_ref = self.state.read(cx).public_mcp_client_editor.clone()?;
        let client = self
            .frame
            .clients
            .iter()
            .find(|client| client.client_ref.as_str() == target_ref)?;

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
        let toggle_key = if client.enabled {
            "settings_view.network.disable_client"
        } else {
            "settings_view.network.enable_client"
        };
        let mode_toggle_key = match client.approval_mode {
            oxideterm_public_mcp::ClientApprovalMode::Standard => {
                "settings_view.network.switch_to_full_mode"
            }
            oxideterm_public_mcp::ClientApprovalMode::Unattended => {
                "settings_view.network.switch_to_standard_mode"
            }
        };

        let mut families = div().w_full().min_w_0().flex().flex_wrap();
        for (title_key, groups) in public_mcp_permission_sections() {
            let selected = groups
                .iter()
                .filter(|group| client.tool_groups.contains(group))
                .count();
            let mut items = Vec::new();
            for &tool_group in groups {
                let checked = client.tool_groups.contains(&tool_group);
                let client_ref = client.client_ref.clone();
                let control = checkbox(
                    &self.tokens,
                    format!("mcp-permission-{client_ref}-{tool_group:?}"),
                    String::new(),
                    checked,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    self.press(McpSettingsRequest::SetToolGroup(
                        client_ref, tool_group, !checked,
                    )),
                )
                .into_any_element();
                items.push(settings_ai_tool_policy_item(
                    &self.tokens,
                    self.i18n.t(public_mcp_tool_group_label_key(tool_group)),
                    control,
                ));
            }
            families = families.child(
                div()
                    .flex_basis(gpui::relative(0.5))
                    .flex_grow(1.0)
                    .min_w(px(280.0))
                    .max_w_full()
                    .pr(px(12.0))
                    .pb(px(12.0))
                    .child(settings_ai_tool_policy_group(
                        &self.tokens,
                        self.i18n.t(title_key),
                        String::new(),
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!("{selected}/{}", groups.len()))
                            .into_any_element(),
                        items,
                    )),
            );
        }

        let body_content = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                self.setting_row(
                    "settings_view.network.mcp_approval_mode",
                    "settings_view.network.mcp_approval_mode_hint",
                    self.workspace_toolbar_action_button(
                        self.i18n.t(mode_toggle_key),
                        None,
                        ToolbarButtonOptions::default(),
                        self.press(McpSettingsRequest::SetMode(
                            client_ref_for_mode,
                            next_approval_mode,
                        )),
                    )
                    .into_any_element(),
                    cx,
                ),
            )
            .child(self.card_separator())
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.network.mcp_group_basic")),
            )
            .child(families);

        let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(PUBLIC_MCP_CLIENT_MODAL_WIDTH))
            .max_w_full()
            .max_h(px(PUBLIC_MCP_CLIENT_MODAL_MAX_HEIGHT))
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, client.label.clone()))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t("settings_view.network.external_clients_hint"),
                    )),
            )
            .child(
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .id("public-mcp-client-editor-body")
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(self.scroll)
                    .vertical_scrollbar(self.scroll)
                    .child(body_content),
            )
            .child(
                dialog_footer(&self.tokens)
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t(toggle_key),
                        None,
                        ToolbarButtonOptions::default(),
                        self.press(McpSettingsRequest::SetEnabled(
                            client_ref_for_toggle,
                            next_enabled,
                        )),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.revoke_client"),
                        Some(render_icon(
                            "lucide/trash-2.svg",
                            SETTINGS_PUBLIC_MCP_ACTION_ICON_SIZE,
                            rgb(self.tokens.ui.error),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                ..Default::default()
                            },
                            text_color: Some(rgb(self.tokens.ui.error)),
                            ..Default::default()
                        },
                        self.press(McpSettingsRequest::RemoveClient(client_ref_for_remove)),
                    ))
                    .child(div().flex_1())
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.terminal.command_specs_close"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Default,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        self.press(McpSettingsRequest::CloseEditor),
                    )),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    self.press(McpSettingsRequest::CloseEditor),
                )
                .child(dialog)
                .into_any_element(),
        )
    }
}

pub struct McpClientDialog {
    body: Rc<dyn Fn(&App) -> Option<AnyElement>>,
    _changed: Subscription,
}

impl McpClientDialog {
    pub fn new(
        state: &Entity<SettingsWorkspaceEntity>,
        body: Rc<dyn Fn(&App) -> Option<AnyElement>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let changed = cx.observe(state, |_, state, cx| {
            if state.read(cx).public_mcp_client_editor.is_some() {
                cx.notify();
            }
        });
        Self {
            body,
            _changed: changed,
        }
    }
}

impl Render for McpClientDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.body)(cx).unwrap_or_else(|| div().into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_permission_sections_preserve_every_selectable_group_once() {
        let mut displayed = public_mcp_permission_sections()
            .into_iter()
            .flat_map(|(_, groups)| groups.iter().copied())
            .collect::<Vec<_>>();
        displayed.sort();
        let mut selectable = oxideterm_public_mcp::ToolGroup::selectable().to_vec();
        selectable.sort();
        assert_eq!(displayed, selectable);
    }
}
