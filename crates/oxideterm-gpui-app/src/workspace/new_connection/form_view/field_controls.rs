use super::*;
use gpui::{Animation, AnimationExt, App};
use oxideterm_settings_model::parse_rgb24_hex;

const NEW_CONNECTION_TRANSPORT_ROW_HEIGHT: f32 = 36.0;
const NEW_CONNECTION_TRANSPORT_ROW_GAP: f32 = 4.0;
const SSH_CONNECT_TIMEOUT_OPTIONS_SECONDS: [u64; 5] = [10, 30, 60, 120, 300];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConnectionFormSection {
    Basic,
    Authentication,
    Route,
    SshOptions,
    Terminal,
    Appearance,
    RemoteGateway,
    VncPreferences,
    RemoteFeatures,
    SerialParameters,
    MoshOptions,
    LocalShell,
}

impl ConnectionFormSection {
    fn element_id(self) -> &'static str {
        match self {
            Self::Basic => "new-connection-basic-section",
            Self::Authentication => "new-connection-authentication-section",
            Self::Route => "new-connection-route-section",
            Self::SshOptions => "new-connection-ssh-options-section",
            Self::Terminal => "new-connection-terminal-section",
            Self::Appearance => "new-connection-appearance-section",
            Self::RemoteGateway => "new-connection-remote-gateway-section",
            Self::VncPreferences => "new-connection-vnc-preferences-section",
            Self::RemoteFeatures => "new-connection-remote-features-section",
            Self::SerialParameters => "new-connection-serial-parameters-section",
            Self::MoshOptions => "new-connection-mosh-options-section",
            Self::LocalShell => "new-connection-local-shell-section",
        }
    }

    fn title_key(self) -> &'static str {
        match self {
            Self::Basic => "ssh.form.basic_information",
            Self::Authentication => "ssh.form.authentication",
            Self::Route => "ssh.form.connection_route",
            Self::SshOptions => "ssh.form.ssh_options",
            Self::Terminal => "ssh.form.terminal_options",
            Self::Appearance => "ssh.form.appearance",
            Self::RemoteGateway => "modals.new_connection.remote_desktop_ssh_gateway",
            Self::VncPreferences => "modals.new_connection.vnc_preferences_title",
            Self::RemoteFeatures => "modals.new_connection.remote_desktop_features_title",
            Self::SerialParameters => "modals.new_connection.serial_section_title",
            Self::MoshOptions => "mosh.form.advanced",
            Self::LocalShell => "settings_view.local_terminal.select_shell",
        }
    }

    fn hint_key(self) -> &'static str {
        match self {
            Self::Basic => "ssh.form.basic_information_hint",
            Self::Authentication => "ssh.form.authentication_hint",
            Self::Route => "ssh.form.connection_route_hint",
            Self::SshOptions => "ssh.form.ssh_options_hint",
            Self::Terminal => "ssh.form.terminal_options_hint",
            Self::Appearance => "ssh.form.appearance_hint",
            Self::RemoteGateway => "modals.new_connection.remote_desktop_ssh_gateway_hint",
            Self::VncPreferences => "modals.new_connection.vnc_preferences_hint",
            Self::RemoteFeatures => "modals.new_connection.remote_desktop_features_hint",
            Self::SerialParameters => "modals.new_connection.serial_connect_hint",
            Self::MoshOptions => "mosh.form.capability_hint",
            Self::LocalShell => "modals.new_connection.local_terminal_detail",
        }
    }
}

fn connection_form_section_expanded_for_form(
    form: &NewConnectionForm,
    section: ConnectionFormSection,
) -> bool {
    // Every section starts open; an explicit user toggle remains authoritative
    // for the lifetime of this form only.
    let override_value = match section {
        ConnectionFormSection::Basic => form.basic_section_expanded,
        ConnectionFormSection::Authentication => form.authentication_section_expanded,
        ConnectionFormSection::Route => form.route_section_expanded,
        ConnectionFormSection::SshOptions => form.ssh_options_section_expanded,
        ConnectionFormSection::Terminal => form.terminal_section_expanded,
        ConnectionFormSection::Appearance => form.appearance_section_expanded,
        ConnectionFormSection::RemoteGateway => form.remote_gateway_section_expanded,
        ConnectionFormSection::VncPreferences => form.vnc_preferences_section_expanded,
        ConnectionFormSection::RemoteFeatures => form.remote_features_section_expanded,
        ConnectionFormSection::SerialParameters => form.serial_parameters_section_expanded,
        ConnectionFormSection::MoshOptions => form.mosh_options_section_expanded,
        ConnectionFormSection::LocalShell => form.local_shell_section_expanded,
    };
    override_value.unwrap_or(true)
}

fn remote_desktop_feature_columns(feature_count: usize) -> u16 {
    // Two columns reduce vertical scanning without squeezing a lone display option.
    if feature_count > 1 { 2 } else { 1 }
}

const REMOTE_DESKTOP_CLIPBOARD_FEATURES: &[(RemoteDesktopSessionFeature, &str, &str)] = &[
    (
        RemoteDesktopSessionFeature::ClipboardText,
        "modals.new_connection.remote_desktop_clipboard_text",
        "modals.new_connection.remote_desktop_clipboard_text_hint",
    ),
    (
        RemoteDesktopSessionFeature::ClipboardImages,
        "modals.new_connection.remote_desktop_clipboard_images",
        "modals.new_connection.remote_desktop_clipboard_images_hint",
    ),
    (
        RemoteDesktopSessionFeature::ClipboardFiles,
        "modals.new_connection.remote_desktop_clipboard_files",
        "modals.new_connection.remote_desktop_clipboard_files_hint",
    ),
];
const REMOTE_DESKTOP_AUDIO_FEATURES: &[(RemoteDesktopSessionFeature, &str, &str)] = &[
    (
        RemoteDesktopSessionFeature::AudioPlayback,
        "modals.new_connection.remote_desktop_audio_playback",
        "modals.new_connection.remote_desktop_audio_playback_hint",
    ),
    (
        RemoteDesktopSessionFeature::AudioCapture,
        "modals.new_connection.remote_desktop_audio_capture",
        "modals.new_connection.remote_desktop_audio_capture_hint",
    ),
];
const REMOTE_DESKTOP_DISPLAY_FEATURES: &[(RemoteDesktopSessionFeature, &str, &str)] = &[(
    RemoteDesktopSessionFeature::MultiMonitor,
    "modals.new_connection.remote_desktop_multi_monitor",
    "modals.new_connection.remote_desktop_multi_monitor_hint",
)];
const RDP_COMPATIBILITY_FEATURES: &[(RemoteDesktopSessionFeature, &str, &str)] = &[(
    RemoteDesktopSessionFeature::DisableRdpGraphicsPipeline,
    "modals.new_connection.remote_desktop_disable_graphics_pipeline",
    "modals.new_connection.remote_desktop_disable_graphics_pipeline_hint",
)];
const VNC_SECURITY_PREFERENCES: &[(RemoteDesktopVncPreference, &str)] = &[
    (
        RemoteDesktopVncPreference::Security(
            RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption,
        ),
        "modals.new_connection.vnc_security_verified",
    ),
    (
        RemoteDesktopVncPreference::Security(
            RemoteDesktopVncSecurityPolicy::AllowUnverifiedEncryption,
        ),
        "modals.new_connection.vnc_security_unverified",
    ),
    (
        RemoteDesktopVncPreference::Security(RemoteDesktopVncSecurityPolicy::AllowLegacy),
        "modals.new_connection.vnc_security_legacy",
    ),
];
const VNC_SESSION_MODE_PREFERENCES: &[(RemoteDesktopVncPreference, &str)] = &[
    (
        RemoteDesktopVncPreference::SessionMode(RemoteDesktopVncSessionMode::Shared),
        "modals.new_connection.vnc_session_shared",
    ),
    (
        RemoteDesktopVncPreference::SessionMode(RemoteDesktopVncSessionMode::Exclusive),
        "modals.new_connection.vnc_session_exclusive",
    ),
];
const VNC_IMAGE_QUALITY_PREFERENCES: &[(RemoteDesktopVncPreference, &str)] = &[
    (
        RemoteDesktopVncPreference::ImageQuality(RemoteDesktopVncImageQuality::Performance),
        "modals.new_connection.vnc_quality_performance",
    ),
    (
        RemoteDesktopVncPreference::ImageQuality(RemoteDesktopVncImageQuality::Balanced),
        "modals.new_connection.vnc_quality_balanced",
    ),
    (
        RemoteDesktopVncPreference::ImageQuality(RemoteDesktopVncImageQuality::BestQuality),
        "modals.new_connection.vnc_quality_best",
    ),
];
const VNC_COMPRESSION_PREFERENCES: &[(RemoteDesktopVncPreference, &str)] = &[
    (
        RemoteDesktopVncPreference::Compression(RemoteDesktopVncCompression::Low),
        "modals.new_connection.vnc_compression_low",
    ),
    (
        RemoteDesktopVncPreference::Compression(RemoteDesktopVncCompression::Balanced),
        "modals.new_connection.vnc_compression_balanced",
    ),
    (
        RemoteDesktopVncPreference::Compression(RemoteDesktopVncCompression::High),
        "modals.new_connection.vnc_compression_high",
    ),
];

fn new_connection_transport_index(transport: NewConnectionTransport) -> usize {
    match transport {
        NewConnectionTransport::Ssh => 0,
        NewConnectionTransport::Mosh => 1,
        NewConnectionTransport::Telnet => 2,
        NewConnectionTransport::Serial => 3,
        NewConnectionTransport::Rdp => 4,
        NewConnectionTransport::Vnc => 5,
        NewConnectionTransport::WslGraphics => 6,
        NewConnectionTransport::LocalTerminal => {
            if cfg!(target_os = "windows") {
                7
            } else {
                6
            }
        }
    }
}

fn new_connection_transport_vertical_offset(
    source: NewConnectionTransport,
    target: NewConnectionTransport,
) -> f32 {
    let row_stride = NEW_CONNECTION_TRANSPORT_ROW_HEIGHT + NEW_CONNECTION_TRANSPORT_ROW_GAP;
    (new_connection_transport_index(source) as f32 - new_connection_transport_index(target) as f32)
        * row_stride
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AuthSelectorContext {
    Standard,
    EditProperties,
    Prompt,
    DrillDown,
    Jump,
}

fn connection_secret_field_value(
    form: &NewConnectionForm,
    field: NewConnectionField,
) -> Option<&str> {
    // Return a view into the Entity-owned draft so rendering cannot create a
    // second credential owner before the text input builds its presentation.
    match field {
        NewConnectionField::Password => Some(&form.password),
        NewConnectionField::Passphrase => Some(&form.passphrase),
        NewConnectionField::UpstreamProxyPassword => Some(&form.upstream_proxy_password),
        NewConnectionField::JumpPassword => form
            .jump_server_form
            .as_ref()
            .map(|jump_form| jump_form.password.as_str()),
        NewConnectionField::JumpPassphrase => form
            .jump_server_form
            .as_ref()
            .map(|jump_form| jump_form.passphrase.as_str()),
        _ => None,
    }
}

impl WorkspaceApp {
    fn connection_form_section_expanded(
        &self,
        section: ConnectionFormSection,
        cx: &mut Context<Self>,
    ) -> bool {
        self.connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| connection_form_section_expanded_for_form(form, section))
            .unwrap_or(false)
    }

    pub(super) fn render_connection_form_section(
        &self,
        section: ConnectionFormSection,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.connection_form_section_expanded(section, cx);
        let element_id = section.element_id();
        let chevron_id = format!("{element_id}-chevron");
        div()
            .flex()
            .flex_col()
            .when(
                !matches!(
                    section,
                    ConnectionFormSection::Basic | ConnectionFormSection::LocalShell
                ),
                |content| {
                    content
                        .border_t_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .pt(px(self.tokens.spacing.three))
                },
            )
            .child(
                div()
                    .id(element_id)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(self.tokens.spacing.three))
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.spacing.one))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t(section.title_key())),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t(section.hint_key())),
                            ),
                    )
                    .child(self.render_animated_chevron(
                        (SharedString::from(chevron_id), expanded as usize),
                        expanded,
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                let Some(form) = state.form.as_mut() else {
                                    return;
                                };
                                let override_value = Some(!expanded);
                                match section {
                                    ConnectionFormSection::Basic => {
                                        form.basic_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::Authentication => {
                                        form.authentication_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::Route => {
                                        form.route_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::SshOptions => {
                                        form.ssh_options_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::Terminal => {
                                        form.terminal_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::Appearance => {
                                        form.appearance_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::RemoteGateway => {
                                        form.remote_gateway_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::VncPreferences => {
                                        form.vnc_preferences_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::RemoteFeatures => {
                                        form.remote_features_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::SerialParameters => {
                                        form.serial_parameters_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::MoshOptions => {
                                        form.mosh_options_section_expanded = override_value;
                                    }
                                    ConnectionFormSection::LocalShell => {
                                        form.local_shell_section_expanded = override_value;
                                    }
                                }
                                form.field_focused = false;
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .when(expanded, |content| {
                content.child(
                    div()
                        .pt(px(self.tokens.spacing.three))
                        .flex()
                        .flex_col()
                        .gap(px(self.tokens.metrics.modal_section_gap))
                        .child(body),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn new_connection_select_anchor_id(
        select_id: NewConnectionSelect,
    ) -> SelectAnchorId {
        match select_id {
            NewConnectionSelect::Group => SelectAnchorId::NewConnectionGroup,
            NewConnectionSelect::KeyAuthSource => SelectAnchorId::NewConnectionKeyAuthSource,
            NewConnectionSelect::ManagedKey => SelectAnchorId::NewConnectionManagedKey,
            NewConnectionSelect::JumpSavedConnection => {
                SelectAnchorId::NewConnectionJumpSavedConnection
            }
            NewConnectionSelect::RemoteDesktopSshGateway => {
                SelectAnchorId::NewConnectionRemoteDesktopSshGateway
            }
            NewConnectionSelect::JumpKeyAuthSource => {
                SelectAnchorId::NewConnectionJumpKeyAuthSource
            }
            NewConnectionSelect::JumpManagedKey => SelectAnchorId::NewConnectionJumpManagedKey,
            NewConnectionSelect::UpstreamProxyPolicy => {
                SelectAnchorId::NewConnectionUpstreamProxyPolicy
            }
            NewConnectionSelect::UpstreamProxyProtocol => {
                SelectAnchorId::NewConnectionUpstreamProxyProtocol
            }
            NewConnectionSelect::UpstreamProxyAuth => {
                SelectAnchorId::NewConnectionUpstreamProxyAuth
            }
            NewConnectionSelect::SerialPort => SelectAnchorId::NewConnectionSerialPort,
            NewConnectionSelect::SerialDataBits => SelectAnchorId::NewConnectionSerialDataBits,
            NewConnectionSelect::SerialStopBits => SelectAnchorId::NewConnectionSerialStopBits,
            NewConnectionSelect::SerialParity => SelectAnchorId::NewConnectionSerialParity,
            NewConnectionSelect::SerialFlowControl => {
                SelectAnchorId::NewConnectionSerialFlowControl
            }
            NewConnectionSelect::TerminalEncoding => SelectAnchorId::NewConnectionTerminalEncoding,
            NewConnectionSelect::TerminalBackspaceSequence => {
                SelectAnchorId::NewConnectionTerminalBackspaceSequence
            }
            NewConnectionSelect::TerminalDeleteSequence => {
                SelectAnchorId::NewConnectionTerminalDeleteSequence
            }
            NewConnectionSelect::TerminalSemanticScheme => {
                SelectAnchorId::NewConnectionTerminalSemanticScheme
            }
            NewConnectionSelect::TerminalHighlightRuleSet => {
                SelectAnchorId::NewConnectionTerminalHighlightRuleSet
            }
        }
    }

    fn new_connection_select_trigger(
        &self,
        select_id: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &Context<Self>,
    ) -> Div {
        let focused = self.connection_form_state(cx).open_select == Some(select_id);
        // New-connection selects live inside modal forms; keep their keyboard
        // focus ring tied to the same browser focus-origin rule as settings
        // and Cloud Sync selects.
        select_trigger_with_focus_visible(
            &self.tokens,
            value,
            placeholder,
            disabled,
            browser_behavior::browser_focus_visible(
                focused,
                self.connection_form_state(cx).select_focus_origin,
            ),
        )
    }

    fn track_new_connection_select_anchor(
        &self,
        select_id: NewConnectionSelect,
        trigger: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = Self::new_connection_select_anchor_id(select_id);
        let anchors = self.connection_flow.read(cx).select_anchor_store();
        let notify_on_change = self.connection_form_state(cx).open_select == Some(select_id);
        let workspace = cx.entity();
        select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
            // Closed triggers only update layout-owned storage. An open popup
            // needs one follow-up root render because its portal position was
            // resolved before this frame's prepaint reported the new bounds.
            if anchors.update(anchor) && notify_on_change {
                let _ = workspace.update(cx, |_this, cx| cx.notify());
            }
        })
        .into_any_element()
    }

    fn open_new_connection_select_from_pointer(
        &mut self,
        select_id: NewConnectionSelect,
        cx: &mut Context<Self>,
    ) {
        // New-connection selects share browser focus-origin semantics with
        // settings selects: pointer-opened menus should not render a keyboard
        // focus-visible ring on the trigger.
        if self.connection_form_state(cx).open_select == Some(select_id) {
            self.close_new_connection_select(cx);
            return;
        }
        self.update_connection_form_state(cx, |state| {
            state.open_select = Some(select_id);
            state.select_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Pointer);
        });
    }

    pub(in crate::workspace) fn close_new_connection_select(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, ConnectionFormState::close_select);
    }

    pub(super) fn clear_new_connection_select_anchor(&mut self, cx: &App) {
        // Every entry in this store belongs to the moving form viewport, so a
        // scroll can invalidate the entire set without enumerating anchor IDs.
        self.connection_flow.read(cx).select_anchor_store().clear();
    }

    pub(super) fn render_connection_hint(&self, text: String) -> AnyElement {
        self.render_connection_hint_with_color(text, self.tokens.ui.text_muted)
    }

    pub(super) fn render_connection_hint_with_color(&self, text: String, color: u32) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(text)
            .into_any_element()
    }

    pub(super) fn render_agent_status(&self, available: Option<bool>) -> AnyElement {
        let (color, label) = match available {
            Some(true) => (
                self.tokens.ui.success,
                self.i18n.t("ssh.form.agent_detected"),
            ),
            Some(false) => (
                self.tokens.ui.error,
                self.i18n.t("ssh.form.agent_not_detected"),
            ),
            None => (self.tokens.ui.text_muted, "...".to_string()),
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(div().size(px(8.0)).rounded_full().bg(rgb(color)))
            .child(div().text_color(rgb(color)).child(label))
            .into_any_element()
    }

    pub(super) fn render_prompt_feedback_box(&self, message: String, success: bool) -> AnyElement {
        let feedback_color = if success {
            self.tokens.ui.success
        } else {
            self.tokens.ui.error
        };
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba(
                (feedback_color << 8) | TAURI_PROMPT_FEEDBACK_BORDER_ALPHA,
            ))
            .bg(rgba((feedback_color << 8) | TAURI_PROMPT_FEEDBACK_ALPHA))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.two))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(feedback_color))
            .child(message)
            .into_any_element()
    }

    pub(super) fn render_connection_field(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let secret_visible = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| connection_secret_field_visible(form, field));
        let input = self.render_connection_input(
            value,
            placeholder,
            field,
            secret && !secret_visible.unwrap_or(false),
            cx,
        );
        let control = if secret {
            self.render_connection_secret_visibility(input, field, secret_visible, cx)
        } else {
            input
        };

        form_field(&self.tokens, label, control)
    }

    pub(super) fn render_connection_secret_field(
        &self,
        label: String,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((input, secret_visible)) =
            self.render_connection_secret_input(placeholder, field, cx)
        else {
            return div().into_any_element();
        };
        let control = self.render_connection_secret_visibility(input, field, secret_visible, cx);
        form_field(&self.tokens, label, control)
    }

    fn render_connection_secret_input(
        &self,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, Option<bool>)> {
        let target = WorkspaceImeTarget::NewConnection(field);
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let marked_text = self.marked_text_for_target(target, cx);
        let caret_visible = self.input_caret.visible();
        let (input, secret_visible) = {
            let form = self.connection_form_state(cx).form.as_ref()?;
            let value = connection_secret_field_value(form, field)?;
            let secret_visible = connection_secret_field_visible(form, field);
            let focused = form.field_focused && form.focused_field == field;
            let selected_all = connection_field_is_selected(form, field);
            let input = text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible,
                    secret: !secret_visible.unwrap_or(false),
                    selected_all,
                    selected_range,
                    marked_text,
                },
            );
            (input, secret_visible)
        };
        let input = self.finish_connection_input(input, field, cx);
        Some((input, secret_visible))
    }

    fn render_connection_secret_visibility(
        &self,
        input: AnyElement,
        field: NewConnectionField,
        secret_visible: Option<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(visible) = secret_visible {
            let icon = if visible {
                LucideIcon::EyeOff
            } else {
                LucideIcon::Eye
            };
            div()
                .relative()
                .child(input)
                .child(
                    self.workspace_icon_action_button(
                        icon,
                        SECRET_VISIBILITY_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                        IconButtonOptions {
                            hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                            ..IconButtonOptions::opaque_toolbar(
                                SECRET_VISIBILITY_BUTTON_SIZE,
                                ButtonRadius::Sm,
                            )
                        },
                        move |this, _event, _window, cx| {
                            let toggled = this.update_connection_form_state(cx, |state| {
                                state.form.as_mut().is_some_and(|form| {
                                    toggle_connection_secret_field_visibility(form, field)
                                })
                            });
                            if toggled {
                                cx.notify();
                            }
                            cx.stop_propagation();
                        },
                        cx,
                    )
                    .absolute()
                    .right(px(SECRET_VISIBILITY_BUTTON_OFFSET))
                    .top(px(SECRET_VISIBILITY_BUTTON_OFFSET)),
                )
                .into_any_element()
        } else {
            input
        }
    }

    pub(super) fn render_edit_saved_password_field(
        &self,
        password_visible: bool,
        password_loading: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((input, _)) = self.render_connection_secret_input(
            self.i18n
                .t("sessionManager.edit_properties.password_placeholder"),
            NewConnectionField::Password,
            cx,
        ) else {
            return div().into_any_element();
        };
        let icon = if password_visible {
            LucideIcon::EyeOff
        } else {
            LucideIcon::Eye
        };
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.saved_password"),
            div().relative().child(input).child(
                if password_loading {
                    oxideterm_gpui_ui::button::icon_button(
                        &self.tokens,
                        self.render_loading_icon(
                            "saved-password-loading",
                            SECRET_VISIBILITY_ICON_SIZE,
                            rgb(self.tokens.ui.text_muted),
                        ),
                        IconButtonOptions {
                            loading: true,
                            hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                            ..IconButtonOptions::opaque_toolbar(
                                SECRET_VISIBILITY_BUTTON_SIZE,
                                ButtonRadius::Sm,
                            )
                        },
                    )
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                } else {
                    self.workspace_icon_action_button(
                        icon,
                        SECRET_VISIBILITY_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                        IconButtonOptions {
                            hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                            ..IconButtonOptions::opaque_toolbar(
                                SECRET_VISIBILITY_BUTTON_SIZE,
                                ButtonRadius::Sm,
                            )
                        },
                        |this, _event, _window, cx| {
                            this.toggle_edit_saved_password_visibility(cx);
                            cx.stop_propagation();
                        },
                        cx,
                    )
                }
                .absolute()
                .right(px(SECRET_VISIBILITY_BUTTON_OFFSET))
                .top(px(SECRET_VISIBILITY_BUTTON_OFFSET)),
            ),
        )
    }

    pub(super) fn render_connection_field_with_browse(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            label,
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(self.render_connection_input(value, placeholder, field, false, cx)),
                )
                .child(
                    // Tauri browse controls are outline Buttons beside the
                    // path input. Keep this modal-form action on the shared
                    // toolbar primitive so disabled/focus behavior can be
                    // centralized with other form buttons.
                    self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.edit_properties.browse"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.pick_new_connection_path(field, cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
        )
    }

    pub(super) fn render_connection_group_select(
        &self,
        label: String,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = if self.connection_form_group_is_ungrouped(value) {
            self.connection_form_ungrouped_label()
        } else {
            value.trim().to_string()
        };
        let trigger = self
            .new_connection_select_trigger(
                NewConnectionSelect::Group,
                selected_label,
                false,
                false,
                cx,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(NewConnectionSelect::Group, cx);
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            label,
            self.track_new_connection_select_anchor(NewConnectionSelect::Group, trigger, cx),
        )
    }

    pub(super) fn set_new_connection_group(&mut self, group: String, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.group = group;
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn render_managed_key_select(
        &self,
        label: String,
        selected_id: &str,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let keys = self.connection_store.managed_ssh_keys();
        let selected_label = if selected_id.trim().is_empty() {
            self.i18n.t("ssh.form.managed_key_placeholder")
        } else {
            keys.iter()
                .find(|key| key.id == selected_id)
                .map(|key| key.name.clone())
                .unwrap_or_else(|| selected_id.to_string())
        };
        let select_id = if jump_form {
            NewConnectionSelect::JumpManagedKey
        } else {
            NewConnectionSelect::ManagedKey
        };
        let trigger = self
            .new_connection_select_trigger(
                select_id,
                selected_label,
                selected_id.trim().is_empty(),
                keys.is_empty(),
                cx,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if this.connection_store.managed_ssh_keys().is_empty() {
                        cx.stop_propagation();
                        return;
                    }
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(select_id, cx);
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            label,
            self.track_new_connection_select_anchor(select_id, trigger, cx),
        )
        .into_any_element()
    }

    pub(super) fn render_jump_saved_connection_select(
        &self,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.connection_store.connection_infos();
        let selected_label = if selected_id.trim().is_empty() {
            self.i18n.t("ssh.form.proxy_jump_saved_connection_custom")
        } else {
            connections
                .iter()
                .find(|connection| connection.id == selected_id)
                .map(|connection| {
                    format!(
                        "{} · {}@{}:{}",
                        connection.name, connection.username, connection.host, connection.port
                    )
                })
                .unwrap_or_else(|| selected_id.to_string())
        };
        let trigger = self
            .new_connection_select_trigger(
                NewConnectionSelect::JumpSavedConnection,
                selected_label,
                selected_id.trim().is_empty(),
                false,
                cx,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(
                        NewConnectionSelect::JumpSavedConnection,
                        cx,
                    );
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            self.i18n.t("ssh.form.proxy_jump_saved_connection"),
            self.track_new_connection_select_anchor(
                NewConnectionSelect::JumpSavedConnection,
                trigger,
                cx,
            ),
        )
        .into_any_element()
    }

    pub(super) fn render_remote_desktop_ssh_gateway_select(
        &self,
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.connection_store.connection_infos();
        let selected_connection = selected_id.and_then(|selected_id| {
            connections
                .iter()
                .find(|connection| connection.id == selected_id)
        });
        let selected_label = match (selected_id, selected_connection) {
            (_, Some(connection)) => format!(
                "{} · {}@{}:{}",
                connection.name, connection.username, connection.host, connection.port
            ),
            (Some(_), None) => self
                .i18n
                .t("modals.new_connection.remote_desktop_ssh_gateway_missing"),
            (None, None) => self
                .i18n
                .t("modals.new_connection.remote_desktop_ssh_gateway_direct"),
        };
        let unavailable = connections.is_empty() && selected_id.is_none();
        let trigger = self
            .new_connection_select_trigger(
                NewConnectionSelect::RemoteDesktopSshGateway,
                selected_label,
                selected_id.is_none(),
                unavailable,
                cx,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let gateway_selected = this
                        .connection_form_state(cx)
                        .form
                        .as_ref()
                        .is_some_and(|form| {
                            form.remote_desktop_ssh_gateway_connection_id.is_some()
                        });
                    if this.connection_store.connections().is_empty() && !gateway_selected {
                        cx.stop_propagation();
                        return;
                    }
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(
                        NewConnectionSelect::RemoteDesktopSshGateway,
                        cx,
                    );
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        // The surrounding route section owns the label and explanatory copy.
        self.track_new_connection_select_anchor(
            NewConnectionSelect::RemoteDesktopSshGateway,
            trigger,
            cx,
        )
    }

    pub(super) fn set_new_connection_managed_key(
        &mut self,
        select_id: NewConnectionSelect,
        key_id: String,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                match select_id {
                    NewConnectionSelect::ManagedKey => {
                        form.managed_key_id = key_id;
                        form.focused_field = NewConnectionField::ManagedKeyId;
                    }
                    NewConnectionSelect::JumpManagedKey => {
                        let Some(jump_form) = form.jump_server_form.as_mut() else {
                            return;
                        };
                        jump_form.managed_key_id = key_id;
                        form.focused_field = NewConnectionField::JumpManagedKeyId;
                    }
                    NewConnectionSelect::Group
                    | NewConnectionSelect::KeyAuthSource
                    | NewConnectionSelect::JumpSavedConnection
                    | NewConnectionSelect::RemoteDesktopSshGateway
                    | NewConnectionSelect::JumpKeyAuthSource
                    | NewConnectionSelect::UpstreamProxyPolicy
                    | NewConnectionSelect::UpstreamProxyProtocol
                    | NewConnectionSelect::UpstreamProxyAuth
                    | NewConnectionSelect::SerialPort
                    | NewConnectionSelect::SerialDataBits
                    | NewConnectionSelect::SerialStopBits
                    | NewConnectionSelect::SerialParity
                    | NewConnectionSelect::SerialFlowControl
                    | NewConnectionSelect::TerminalEncoding
                    | NewConnectionSelect::TerminalBackspaceSequence
                    | NewConnectionSelect::TerminalDeleteSequence
                    | NewConnectionSelect::TerminalSemanticScheme
                    | NewConnectionSelect::TerminalHighlightRuleSet => return,
                }
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn clear_new_connection_jump_saved_connection(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                if let Some(jump_form) = form.jump_server_form.as_mut() {
                    jump_form.saved_connection_id.clear();
                }
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_jump_saved_connection(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let selected_connection = self
            .connection_store
            .connection_infos()
            .into_iter()
            .find(|connection| connection.id == connection_id);
        if let Some(connection) = selected_connection.as_ref() {
            self.update_connection_form_state(cx, |state| {
                let Some(form) = state.form.as_mut() else {
                    return;
                };
                if let Some(jump_form) = form.jump_server_form.as_mut() {
                    jump_form.apply_saved_connection(connection);
                }
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            });
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn connection_form_group_options(&self, current_group: &str) -> Vec<String> {
        let mut groups = self.connection_store.groups().to_vec();
        let current = current_group.trim();
        if !current.is_empty()
            && !self.connection_form_group_is_ungrouped(current)
            && !groups.iter().any(|group| group == current)
        {
            groups.push(current.to_string());
        }
        groups.sort();
        groups.dedup();
        groups
    }

    pub(super) fn connection_form_group_is_ungrouped(&self, group: &str) -> bool {
        let group = group.trim();
        group.is_empty()
            || group == "Ungrouped"
            || group == "未分组"
            || group == self.i18n.t("ssh.form.ungrouped")
            || group == self.i18n.t("sessionManager.edit_properties.ungrouped")
    }

    pub(super) fn connection_form_ungrouped_label(&self) -> String {
        self.i18n.t("ssh.form.ungrouped")
    }

    pub(super) fn render_proxy_command_section(
        &self,
        enabled: bool,
        command: &str,
        configured: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(self.render_connection_checkbox(
                self.i18n.t("ssh.form.proxy_command_enable"),
                enabled,
                |form| form.proxy_command_enabled = !form.proxy_command_enabled,
                cx,
            ))
            .when(enabled, |content| {
                content
                    .child(self.render_connection_field(
                        self.i18n.t("ssh.form.proxy_command"),
                        command,
                        self.i18n.t(if configured && command.is_empty() {
                            "ssh.form.proxy_command_configured_placeholder"
                        } else {
                            "ssh.form.proxy_command_placeholder"
                        }),
                        NewConnectionField::ProxyCommand,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.proxy_command_hint")))
            })
            .into_any_element()
    }

    fn pick_new_connection_path(&mut self, field: NewConnectionField, cx: &mut Context<Self>) {
        if !matches!(
            field,
            NewConnectionField::KeyPath
                | NewConnectionField::CertPath
                | NewConnectionField::JumpKeyPath
                | NewConnectionField::JumpCertPath
        ) {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("sessionManager.edit_properties.browse"),
            )),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            let Some(path) = paths.into_iter().next() else {
                return None;
            };
            Some(path.to_string_lossy().to_string())
        };
        self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.start_path_picker(field, selection, cx);
        });
    }

    fn toggle_edit_saved_password_visibility(&mut self, cx: &mut Context<Self>) {
        let source_connection_id = self
            .saved_connection_form_source_id(cx)
            .map(|connection_id| connection_id.to_string());
        let should_load = self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return false;
            };
            if form.password_loading {
                return false;
            }
            if form.password_loaded {
                form.password_visible = !form.password_visible;
                form.password_error = None;
                return false;
            }
            form.password_loading = true;
            form.password_error = None;
            true
        });
        if !should_load {
            cx.notify();
            return;
        }
        let Some(connection_id) = source_connection_id else {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.password_loading = false;
                }
            });
            return;
        };
        cx.notify();

        let store = self.connection_store.clone();
        let load = async move { store.get_connection_password(&connection_id) };
        self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.start_password_load(load, cx);
        });
    }

    fn render_connection_input(
        &self,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.field_focused && form.focused_field == field);
        let selected_all = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| connection_field_is_selected(form, field));
        let target = WorkspaceImeTarget::NewConnection(field);
        let input = text_input(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: self.input_caret.visible(),
                secret,
                selected_all,
                selected_range: self.ime_selected_range_for_target(target, cx),
                marked_text: self.marked_text_for_target(target, cx),
            },
        );
        self.finish_connection_input(input, field, cx)
    }

    fn finish_connection_input(
        &self,
        input: Div,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::NewConnection(field);
        let anchors = self.text_input_anchors.clone();
        text_input_anchor_probe(
            target.anchor_id(),
            input
                .id(("connection-field", field as u32))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.field_focused = true;
                                form.focused_field = field;
                                clear_connection_selection(form);
                            }
                        });
                        this.close_new_connection_select(cx);
                        this.ime_marked_text = None;
                        this.show_active_input_caret(cx);
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(
                    cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    }),
                ),
            move |anchor, _window, _cx| {
                // Text geometry is layout-only state. Writing the shared store
                // directly avoids re-entering WorkspaceApp once per input on
                // every frame of an inertial form scroll.
                anchors.update(anchor);
            },
        )
        .into_any_element()
    }

    pub(super) fn render_auth_selector(
        &self,
        active_tab: SshAuthTab,
        context: AuthSelectorContext,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_family = auth_family_from_tab(active_tab);
        let label = match context {
            AuthSelectorContext::EditProperties => {
                self.i18n.t("sessionManager.edit_properties.auth_type")
            }
            AuthSelectorContext::DrillDown => self.i18n.t("ssh.drill_down.auth_method"),
            AuthSelectorContext::Jump => self.i18n.t("ssh.form.proxy_jump_auth"),
            AuthSelectorContext::Standard | AuthSelectorContext::Prompt => {
                self.i18n.t("ssh.form.authentication")
            }
        };
        let choices = Self::auth_family_choices(context);
        let active_index = choices
            .iter()
            .position(|(family, _)| *family == active_family)
            .unwrap_or(0);
        let control_id = Self::auth_selector_motion_id(context);
        let previous_index = self
            .segmented_control_user_previous_index(control_id, active_index)
            .unwrap_or(active_index);
        let transition_generation = self
            .segmented_control_user_transition(control_id, active_index)
            .map(|(generation, _)| generation);
        let mut items = Vec::with_capacity(choices.len());
        for (choice_index, (family, label_key)) in choices.iter().enumerate() {
            let family = *family;
            let item = segmented_tab(
                &self.tokens,
                self.i18n.t(label_key),
                family == active_family,
            )
            // The moving surface owns the selected background; the trigger keeps
            // the exact legacy typography, spacing, and inactive appearance.
            .bg(rgba(0x00000000))
            .min_h(px(self.tokens.metrics.ui_tabs_list_height))
            .whitespace_normal()
            .text_align(gpui::TextAlign::Center)
            .line_height(px(self.tokens.metrics.ui_text_sm + 2.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.set_new_connection_auth_family(family, context, jump_form, cx);
                    if choice_index != active_index {
                        this.begin_user_segmented_control_transition_from(
                            control_id,
                            active_index,
                            choice_index,
                            cx,
                        );
                    }
                }),
            );
            items.push(item.into_any_element());
        }
        let item_width = 1.0 / choices.len().max(1) as f32;
        let active_left = active_index as f32 * item_width;
        let previous_left = previous_index as f32 * item_width;
        let indicator = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(gpui::relative(item_width))
            .rounded(px(self.tokens.radii.xs))
            .bg(rgb(self.tokens.ui.bg));
        let indicator = match (
            transition_generation,
            oxideterm_gpui_ui::segmented_control_motion(&self.tokens),
        ) {
            (Some(generation), Some(motion)) if motion.spatial => indicator
                .with_animation(
                    (
                        gpui::ElementId::from(control_id),
                        format!("selection-{generation}"),
                    ),
                    Animation::new(motion.duration)
                        .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                    move |indicator, progress| {
                        indicator.left(gpui::relative(oxideterm_gpui_ui::motion::lerp(
                            previous_left,
                            active_left,
                            progress,
                        )))
                    },
                )
                .into_any_element(),
            (Some(generation), Some(motion)) => indicator
                .left(gpui::relative(active_left))
                .with_animation(
                    (
                        gpui::ElementId::from(control_id),
                        format!("selection-{generation}"),
                    ),
                    Animation::new(motion.duration),
                    |indicator, progress| indicator.opacity(progress),
                )
                .into_any_element(),
            _ => indicator
                .left(gpui::relative(active_left))
                .into_any_element(),
        };
        let mut inner = div().relative().w_full().flex().flex_row().child(indicator);
        for item in items {
            inner = inner.child(item);
        }
        // Preserve the original authentication selector shell exactly; only
        // its selected fill moves between the existing option cells.
        let row = segmented_tabs(&self.tokens).child(inner);

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(form_field(&self.tokens, label, row))
            .when(
                active_family == SshAuthFamily::Key && context != AuthSelectorContext::DrillDown,
                |content| {
                    content.child(
                        self.render_key_auth_source_select(active_tab, context, jump_form, cx),
                    )
                },
            )
            .into_any_element()
    }

    fn auth_family_choices(
        context: AuthSelectorContext,
    ) -> &'static [(SshAuthFamily, &'static str)] {
        match context {
            AuthSelectorContext::DrillDown => &[
                (SshAuthFamily::Agent, "ssh.drill_down.auth_agent"),
                (SshAuthFamily::Key, "ssh.drill_down.auth_key"),
                (SshAuthFamily::Password, "ssh.drill_down.auth_password"),
            ],
            AuthSelectorContext::Jump => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
            ],
            AuthSelectorContext::EditProperties | AuthSelectorContext::Prompt => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
            ],
            AuthSelectorContext::Standard => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
                (SshAuthFamily::TwoFactor, "ssh.auth.two_factor"),
            ],
        }
    }

    fn auth_selector_motion_id(context: AuthSelectorContext) -> &'static str {
        match context {
            AuthSelectorContext::Standard => {
                crate::workspace::selection_motion::NEW_CONNECTION_AUTH_SELECTOR_ID
            }
            AuthSelectorContext::EditProperties => {
                crate::workspace::selection_motion::EDIT_CONNECTION_AUTH_SELECTOR_ID
            }
            AuthSelectorContext::Prompt => {
                crate::workspace::selection_motion::PROMPT_CONNECTION_AUTH_SELECTOR_ID
            }
            AuthSelectorContext::DrillDown => {
                crate::workspace::selection_motion::DRILL_DOWN_AUTH_SELECTOR_ID
            }
            AuthSelectorContext::Jump => {
                crate::workspace::selection_motion::JUMP_CONNECTION_AUTH_SELECTOR_ID
            }
        }
    }

    pub(super) fn key_auth_source_choices(
        context: AuthSelectorContext,
    ) -> &'static [SshKeyAuthSource] {
        match context {
            AuthSelectorContext::Standard | AuthSelectorContext::Jump => &[
                SshKeyAuthSource::DefaultKey,
                SshKeyAuthSource::SshKey,
                SshKeyAuthSource::ManagedKey,
                SshKeyAuthSource::Certificate,
            ],
            AuthSelectorContext::EditProperties | AuthSelectorContext::Prompt => &[
                SshKeyAuthSource::SshKey,
                SshKeyAuthSource::ManagedKey,
                SshKeyAuthSource::Certificate,
            ],
            AuthSelectorContext::DrillDown => &[SshKeyAuthSource::SshKey],
        }
    }

    pub(super) fn current_main_auth_selector_context(
        &self,
        cx: &Context<Self>,
    ) -> AuthSelectorContext {
        let mode = self.connection_form_state(cx).mode();
        if self
            .connection_form_state(cx)
            .drill_down_parent_node_id
            .is_some()
        {
            AuthSelectorContext::DrillDown
        } else if mode == NewConnectionFormMode::SavedConnectionPrompt {
            AuthSelectorContext::Prompt
        } else if mode == NewConnectionFormMode::EditProperties {
            AuthSelectorContext::EditProperties
        } else {
            AuthSelectorContext::Standard
        }
    }

    pub(super) fn key_auth_source_label(&self, source: SshKeyAuthSource) -> String {
        let key = match source {
            SshKeyAuthSource::DefaultKey => "ssh.auth.key_source_default",
            SshKeyAuthSource::SshKey => "ssh.auth.key_source_file",
            SshKeyAuthSource::ManagedKey => "ssh.auth.key_source_managed",
            SshKeyAuthSource::Certificate => "ssh.auth.key_source_certificate",
        };
        self.i18n.t(key)
    }

    pub(super) fn normalized_key_source_for_context(
        active_tab: SshAuthTab,
        context: AuthSelectorContext,
    ) -> SshKeyAuthSource {
        let choices = Self::key_auth_source_choices(context);
        let source = key_source_from_tab(active_tab).unwrap_or(SshKeyAuthSource::SshKey);
        if choices.contains(&source) {
            source
        } else {
            SshKeyAuthSource::SshKey
        }
    }

    fn render_key_auth_source_select(
        &self,
        active_tab: SshAuthTab,
        context: AuthSelectorContext,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = Self::normalized_key_source_for_context(active_tab, context);
        let select_id = if jump_form {
            NewConnectionSelect::JumpKeyAuthSource
        } else {
            NewConnectionSelect::KeyAuthSource
        };
        let trigger = self
            .new_connection_select_trigger(
                select_id,
                self.key_auth_source_label(source),
                false,
                false,
                cx,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(select_id, cx);
                    window.focus(&this.focus_handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            self.i18n.t("ssh.auth.key_source"),
            self.track_new_connection_select_anchor(select_id, trigger, cx),
        )
        .into_any_element()
    }

    fn set_new_connection_auth_family(
        &mut self,
        family: SshAuthFamily,
        context: AuthSelectorContext,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                let current_tab = if jump_form {
                    form.jump_server_form
                        .as_ref()
                        .map(|jump_form| jump_form.auth_tab)
                        .unwrap_or(SshAuthTab::Password)
                } else {
                    form.auth_tab
                };
                let next_tab = Self::auth_tab_for_family_selection(family, current_tab, context);
                if jump_form {
                    if let Some(jump_form) = form.jump_server_form.as_mut() {
                        jump_form.auth_tab = next_tab;
                    }
                } else {
                    form.auth_tab = next_tab;
                }
                form.focused_field = Self::focus_field_for_auth_tab(next_tab, jump_form);
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.close_new_connection_select(cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    fn auth_tab_for_family_selection(
        family: SshAuthFamily,
        current_tab: SshAuthTab,
        context: AuthSelectorContext,
    ) -> SshAuthTab {
        match family {
            SshAuthFamily::Password => SshAuthTab::Password,
            SshAuthFamily::Agent => SshAuthTab::Agent,
            SshAuthFamily::TwoFactor => SshAuthTab::TwoFactor,
            SshAuthFamily::Key => {
                // A top-level switch into Key should land on the file-key form,
                // while repeated clicks preserve the selected key source.
                if auth_family_from_tab(current_tab) == SshAuthFamily::Key {
                    auth_tab_from_key_source(Self::normalized_key_source_for_context(
                        current_tab,
                        context,
                    ))
                } else {
                    default_auth_tab_for_family(family)
                }
            }
        }
    }

    pub(super) fn set_new_connection_key_auth_source(
        &mut self,
        select_id: NewConnectionSelect,
        source: SshKeyAuthSource,
        cx: &mut Context<Self>,
    ) {
        let tab = auth_tab_from_key_source(source);
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                match select_id {
                    NewConnectionSelect::KeyAuthSource => form.auth_tab = tab,
                    NewConnectionSelect::JumpKeyAuthSource => {
                        let Some(jump_form) = form.jump_server_form.as_mut() else {
                            return;
                        };
                        jump_form.auth_tab = tab;
                    }
                    _ => return,
                }
                form.focused_field = Self::focus_field_for_auth_tab(
                    tab,
                    select_id == NewConnectionSelect::JumpKeyAuthSource,
                );
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    fn focus_field_for_auth_tab(tab: SshAuthTab, jump_form: bool) -> NewConnectionField {
        if jump_form {
            match tab {
                SshAuthTab::Password => NewConnectionField::JumpPassword,
                SshAuthTab::SshKey | SshAuthTab::Certificate => NewConnectionField::JumpKeyPath,
                SshAuthTab::ManagedKey => NewConnectionField::JumpManagedKeyId,
                SshAuthTab::DefaultKey | SshAuthTab::Agent | SshAuthTab::TwoFactor => {
                    NewConnectionField::JumpHost
                }
            }
        } else {
            match tab {
                SshAuthTab::Password => NewConnectionField::Password,
                SshAuthTab::SshKey | SshAuthTab::Certificate => NewConnectionField::KeyPath,
                SshAuthTab::ManagedKey => NewConnectionField::ManagedKeyId,
                SshAuthTab::DefaultKey => NewConnectionField::Passphrase,
                SshAuthTab::Agent | SshAuthTab::TwoFactor => NewConnectionField::Host,
            }
        }
    }

    pub(super) fn render_edit_color_field(
        &self,
        label: String,
        value: &str,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let swatch = parse_rgb24_hex(value).unwrap_or(TAURI_EDIT_COLOR_FALLBACK);
        form_field(
            &self.tokens,
            label,
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(self.tokens.metrics.form_input_height))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .bg(rgb(swatch)),
                )
                .child(div().flex_1().child(self.render_connection_input(
                    value,
                    TAURI_EDIT_COLOR_FALLBACK_TEXT.to_string(),
                    field,
                    false,
                    cx,
                )))
                .when(!value.is_empty(), |row| {
                    row.child(
                        button(
                            &self.tokens,
                            self.i18n.t("sessionManager.edit_properties.clear_color"),
                            ButtonTone::Secondary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        match field {
                                            NewConnectionField::Color => form.color.clear(),
                                            NewConnectionField::IconBackgroundColor => {
                                                form.icon_background_color.clear()
                                            }
                                            _ => {}
                                        }
                                        clear_connection_selection(form);
                                    }
                                });
                                cx.notify();
                            }),
                        ),
                    )
                }),
        )
    }

    pub(super) fn render_edit_icon_field(
        &self,
        icon_value: &str,
        color_value: &str,
        background_color_value: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let preview_color = parse_rgb24_hex(color_value).unwrap_or(theme.accent);
        let preview_background = parse_rgb24_hex(background_color_value)
            .map(rgb)
            .unwrap_or_else(|| rgba((preview_color << 8) | 0x22));
        let active_icon = session_icon_from_id(Some(icon_value)).unwrap_or(LucideIcon::Server);
        let mut grid = div().flex().flex_wrap().gap(px(self.tokens.spacing.two));

        for choice in SESSION_ICON_CHOICES {
            let selected = icon_value.trim() == choice.id;
            let icon_id = choice.id.to_string();
            grid = grid.child(
                div()
                    .size(px(38.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x22)
                    } else {
                        rgb(theme.bg)
                    })
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        choice.icon,
                        18.0,
                        if selected {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.icon = icon_id.clone();
                                    clear_connection_selection(form);
                                }
                            });
                            cx.notify();
                        }),
                    ),
            );
        }

        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.icon"),
            div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.spacing.three))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(self.tokens.spacing.three))
                        .child(
                            div()
                                .size(px(self.tokens.metrics.form_input_height))
                                .rounded(px(self.tokens.radii.md))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .bg(preview_background)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Self::render_lucide_icon(
                                    active_icon,
                                    18.0,
                                    rgb(preview_color),
                                )),
                        )
                        .child(
                            button(
                                &self.tokens,
                                if expanded {
                                    self.i18n.t("sessionManager.edit_properties.hide_icons")
                                } else {
                                    self.i18n.t("sessionManager.edit_properties.choose_icon")
                                },
                                ButtonTone::Secondary,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.update_connection_form_state(cx, |state| {
                                        if let Some(form) = state.form.as_mut() {
                                            form.icon_picker_expanded = !form.icon_picker_expanded;
                                            clear_connection_selection(form);
                                        }
                                    });
                                    cx.notify();
                                }),
                            ),
                        )
                        .when(!icon_value.trim().is_empty(), |row| {
                            row.child(
                                button(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.default_icon"),
                                    ButtonTone::Secondary,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.update_connection_form_state(cx, |state| {
                                            if let Some(form) = state.form.as_mut() {
                                                form.icon.clear();
                                                clear_connection_selection(form);
                                            }
                                        });
                                        cx.notify();
                                    }),
                                ),
                            )
                        }),
                )
                .when(expanded, |content| {
                    content.child(
                        div()
                            .id("edit-connection-icon-grid")
                            .max_h(px(180.0))
                            .selectable_overflow_y_scroll(
                                &self.selectable_text_scroll_handle("edit-connection-icon-grid"),
                            )
                            // The icon grid is a nested scroll surface inside
                            // the edit dialog. Wheel input over it should not
                            // also move the outer form body.
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(grid),
                    )
                }),
        )
    }

    pub(super) fn render_transport_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active_transport = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.transport)
            .unwrap_or(NewConnectionTransport::Ssh);
        let mut choices = vec![
            (
                NewConnectionTransport::Ssh,
                self.i18n.t("modals.new_connection.transport_ssh"),
                NewConnectionField::Name,
                LucideIcon::Server,
            ),
            (
                NewConnectionTransport::Mosh,
                self.i18n.t("modals.new_connection.transport_mosh"),
                NewConnectionField::Name,
                LucideIcon::Wifi,
            ),
            (
                NewConnectionTransport::Telnet,
                self.i18n.t("modals.new_connection.transport_telnet"),
                NewConnectionField::Host,
                LucideIcon::Network,
            ),
            (
                NewConnectionTransport::Serial,
                self.i18n.t("modals.new_connection.transport_serial"),
                NewConnectionField::SerialPortPath,
                LucideIcon::Radio,
            ),
            (
                NewConnectionTransport::Rdp,
                self.i18n.t("modals.new_connection.transport_rdp"),
                NewConnectionField::Host,
                LucideIcon::Monitor,
            ),
            (
                NewConnectionTransport::Vnc,
                self.i18n.t("modals.new_connection.transport_vnc"),
                NewConnectionField::Host,
                LucideIcon::Monitor,
            ),
        ];
        if cfg!(target_os = "windows") {
            choices.push((
                NewConnectionTransport::WslGraphics,
                self.i18n.t("modals.new_connection.transport_wsl_graphics"),
                NewConnectionField::Name,
                LucideIcon::AppWindow,
            ));
        }
        // Local terminals are one-shot launch targets, so keep them after saved transports.
        choices.push((
            NewConnectionTransport::LocalTerminal,
            self.i18n
                .t("modals.new_connection.transport_local_terminal"),
            NewConnectionField::Name,
            LucideIcon::Terminal,
        ));
        let mut sidebar = div()
            .w(px(NEW_CONNECTION_TYPE_SIDEBAR_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .pr(px(self.tokens.spacing.three));

        for (transport, label, focus_field, icon) in choices {
            let active = active_transport == transport;
            let transport_index = new_connection_transport_index(transport);
            let row_text = if active {
                theme.text_heading
            } else {
                theme.text
            };
            let icon_color = if active {
                theme.accent
            } else {
                theme.text_muted
            };
            let selection_transition = active.then_some(()).and_then(|()| {
                self.segmented_control_user_transition(
                    crate::workspace::selection_motion::NEW_CONNECTION_TRANSPORT_SELECTOR_ID,
                    transport_index,
                )
            });
            let selection_surface = active.then(|| {
                let surface = div()
                    .absolute()
                    .inset_0()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(self.settings_panel_background(theme.bg_panel));
                let surface = oxideterm_gpui_ui::theme_card_surface_shadow(surface, &self.tokens);

                let Some((generation, vertical_offset_y)) = selection_transition else {
                    return surface.into_any_element();
                };
                let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
                    return surface.into_any_element();
                };
                let animation_id = (
                    gpui::ElementId::from(
                        crate::workspace::selection_motion::NEW_CONNECTION_TRANSPORT_SELECTOR_ID,
                    ),
                    format!("selection-{generation}"),
                );

                if motion.spatial
                    && let Some(vertical_offset_y) = vertical_offset_y
                {
                    return surface
                        .with_animation(
                            animation_id,
                            Animation::new(motion.duration)
                                .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                            move |surface, progress| {
                                let offset = oxideterm_gpui_ui::motion::lerp(
                                    vertical_offset_y,
                                    0.0,
                                    progress,
                                );
                                // Move both edges so the highlight keeps its
                                // fixed row height during vertical travel.
                                surface.top(px(offset)).bottom(px(-offset))
                            },
                        )
                        .into_any_element();
                }

                surface
                    .with_animation(
                        animation_id,
                        Animation::new(motion.duration)
                            .with_easing(oxideterm_gpui_ui::motion::ease_out_cubic),
                        |surface, progress| surface.opacity(progress),
                    )
                    .into_any_element()
            });
            let row = div()
                .w_full()
                .h(px(NEW_CONNECTION_TRANSPORT_ROW_HEIGHT))
                .flex_none()
                .relative()
                .flex()
                .items_center()
                .gap(px(self.tokens.spacing.two))
                .px(px(self.tokens.spacing.two))
                .cursor_pointer()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(row_text))
                .when(!active, |row| {
                    row.hover(|row| row.bg(rgba((theme.bg_hover << 8) | 0x80)))
                })
                .when_some(selection_surface, |row, surface| row.child(surface))
                .child(Self::render_lucide_icon(icon, 14.0, rgb(icon_color)))
                .child(div().min_w(px(0.0)).truncate().child(label))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let mut should_refresh_ports = false;
                        let mut selection_offset = None;
                        this.update_connection_form_state(cx, |state| {     if let Some(form) = state.form.as_mut() {
                            let previous_transport = form.transport;
                            if previous_transport != transport {
                                selection_offset = Some(new_connection_transport_vertical_offset(
                                    previous_transport,
                                    transport,
                                ));
                            }
                            apply_transport_default_port(form, previous_transport, transport);
                            apply_transport_default_username(form, previous_transport, transport);
                            form.transport = transport;
                            form.focused_field = focus_field;
                            form.field_focused = false;
                            form.error = None;
                            clear_connection_selection(form);
                            should_refresh_ports = transport == NewConnectionTransport::Serial
                                && form.serial_ports.is_empty()
                                && !form.serial_ports_loading;
                            } });
                        if let Some(vertical_offset_y) = selection_offset {
                            this.begin_user_segmented_control_transition_with_vertical_offset(
                                crate::workspace::selection_motion::NEW_CONNECTION_TRANSPORT_SELECTOR_ID,
                                transport_index,
                                Some(vertical_offset_y),
                                cx,
                            );
                        }
                        this.close_new_connection_select(cx);
                        if should_refresh_ports {
                            this.refresh_serial_ports(cx);
                        }
                        cx.notify();
                    }),
                );
            sidebar = sidebar.child(row);
        }
        sidebar.into_any_element()
    }

    pub(super) fn render_local_terminal_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_shell_id = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.local_shell_id.as_deref());
        let resolved_shell = self.resolved_local_shell(selected_shell_id);
        let resolved_shell_id = resolved_shell.as_ref().map(|shell| shell.id.as_str());
        let default_shell_id = self
            .settings_store
            .settings()
            .local_terminal
            .default_shell_id
            .as_deref();
        let shells = self.effective_local_shells_for_settings(self.settings_store.settings());

        let mut shell_list = div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .flex()
            .flex_col()
            .overflow_hidden();
        for (index, shell) in shells.into_iter().enumerate() {
            let shell_id = shell.id.clone();
            let selected = resolved_shell_id == Some(shell.id.as_str());
            let is_default = default_shell_id == Some(shell.id.as_str());
            shell_list = shell_list.child(
                div()
                    .w_full()
                    .min_w_0()
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(rgb(theme.border))
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x14)
                    } else {
                        rgba((theme.bg_panel << 8) | 0x00)
                    })
                    .hover(move |row| row.bg(rgb(theme.bg_hover)))
                    .px(px(self.tokens.spacing.three))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.three))
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        17.0,
                        rgb(if selected {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(shell.label)
                                    .when(is_default, |label| {
                                        label.child(self.text_badge(
                                            self.i18n.t("settings_view.local_terminal.default"),
                                            theme.warning,
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(shell.path.display().to_string()),
                            ),
                    )
                    .when(selected, |row| {
                        row.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            16.0,
                            rgb(theme.accent),
                        ))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.local_shell_id = Some(shell_id.clone());
                                    form.field_focused = false;
                                    clear_connection_selection(form);
                                    form.error = None;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        // Local terminals have one meaningful choice and inherit all other
        // behavior from application settings, so keep one collapsible section.
        self.render_connection_form_section(
            ConnectionFormSection::LocalShell,
            shell_list.into_any_element(),
            cx,
        )
    }

    pub(super) fn render_wsl_graphics_form_branch(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(self.tokens.spacing.three))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AppWindow,
                        18.0,
                        rgb(theme.accent),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("modals.new_connection.transport_wsl_graphics")),
                    ),
            )
            .child(
                self.render_connection_hint(
                    self.i18n.t("modals.new_connection.wsl_graphics_detail"),
                ),
            )
            .when(!cfg!(target_os = "windows"), |panel| {
                panel.child(
                    self.render_connection_hint_with_color(
                        self.i18n
                            .t("modals.new_connection.wsl_graphics_windows_only"),
                        theme.error,
                    ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_mosh_advanced_fields(
        &self,
        server_executable: &str,
        udp_host: &str,
        udp_port: &str,
        locale: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (ip_family, prediction) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| (form.mosh_ip_family, form.mosh_prediction))
            .unwrap_or_default();
        let ip_family_options = [
            (MoshIpFamily::Auto, "mosh.form.ip_auto"),
            (MoshIpFamily::Ipv4, "mosh.form.ipv4"),
            (MoshIpFamily::Ipv6, "mosh.form.ipv6"),
        ];
        let prediction_options = [
            (
                MoshPredictionMode::Adaptive,
                "mosh.form.prediction_adaptive",
            ),
            (MoshPredictionMode::Always, "mosh.form.prediction_always"),
            (MoshPredictionMode::Never, "mosh.form.prediction_never"),
        ];

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(self.render_connection_field(
                self.i18n.t("mosh.form.server_executable"),
                server_executable,
                "mosh-server".to_string(),
                NewConnectionField::MoshServerExecutable,
                false,
                cx,
            ))
            .child(self.render_connection_field(
                self.i18n.t("mosh.form.udp_host"),
                udp_host,
                self.i18n.t("mosh.form.udp_host_placeholder"),
                NewConnectionField::MoshUdpHost,
                false,
                cx,
            ))
            .child(self.render_connection_field(
                self.i18n.t("mosh.form.udp_port"),
                udp_port,
                self.i18n.t("mosh.form.udp_port_placeholder"),
                NewConnectionField::MoshUdpPort,
                false,
                cx,
            ))
            .child(self.render_connection_hint(self.i18n.t("mosh.form.udp_port_hint")))
            .child(self.render_connection_field(
                self.i18n.t("mosh.form.locale"),
                locale,
                self.i18n.t("mosh.form.locale_placeholder"),
                NewConnectionField::MoshLocale,
                false,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(self.i18n.t("mosh.form.ip_family"))
                    .child(div().flex().gap(px(self.tokens.spacing.one)).children(
                        ip_family_options.into_iter().enumerate().map(
                            |(index, (value, label_key))| {
                                segmented_tab(
                                    &self.tokens,
                                    self.i18n.t(label_key),
                                    ip_family == value,
                                )
                                .id(SharedString::from(format!("mosh-ip-family-{index}")))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.update_connection_form_state(cx, |state| {
                                            if let Some(form) = state.form.as_mut() {
                                                form.mosh_ip_family = value;
                                            }
                                        });
                                        cx.notify();
                                    }),
                                )
                            },
                        ),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(self.i18n.t("mosh.form.prediction"))
                    .child(div().flex().gap(px(self.tokens.spacing.one)).children(
                        prediction_options.into_iter().enumerate().map(
                            |(index, (value, label_key))| {
                                segmented_tab(
                                    &self.tokens,
                                    self.i18n.t(label_key),
                                    prediction == value,
                                )
                                .id(SharedString::from(format!("mosh-prediction-{index}")))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.update_connection_form_state(cx, |state| {
                                            if let Some(form) = state.form.as_mut() {
                                                form.mosh_prediction = value;
                                            }
                                        });
                                        cx.notify();
                                    }),
                                )
                            },
                        ),
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_remote_desktop_form_branch(
        &self,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((
            name,
            host,
            port,
            username,
            keeps_saved_password,
            save_password,
            group,
            ssh_gateway_connection_id,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.name.clone(),
                form.host.clone(),
                form.port.clone(),
                form.username.clone(),
                form.remote_desktop_profile_id.is_some()
                    && form.saved_password_keychain_id.is_some(),
                form.save_password,
                form.group.clone(),
                form.remote_desktop_ssh_gateway_connection_id.clone(),
            )
        })
        else {
            return div().into_any_element();
        };
        let port_placeholder = match protocol {
            oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp => RDP_DEFAULT_PORT_TEXT,
            oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc => VNC_DEFAULT_PORT_TEXT,
        };
        let port_invalid =
            !port.trim().is_empty() && !port.trim().parse::<u16>().is_ok_and(|port| port > 0);
        let capabilities =
            oxideterm_remote_desktop::builtin_provider_manifest(protocol).capabilities;
        let basic = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_field(
                self.i18n.t("ssh.form.name"),
                &name,
                self.i18n.t("ssh.form.name_placeholder"),
                NewConnectionField::Name,
                false,
                cx,
            ))
            .child(self.render_connection_group_select(self.i18n.t("ssh.form.group"), &group, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(div().flex_1().child(self.render_connection_field(
                        self.i18n.t("ssh.form.host"),
                        &host,
                        self.i18n.t("ssh.form.host_placeholder"),
                        NewConnectionField::Host,
                        false,
                        cx,
                    )))
                    .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                        self.render_connection_field(
                            self.i18n.t("ssh.form.port"),
                            &port,
                            port_placeholder.to_string(),
                            NewConnectionField::Port,
                            false,
                            cx,
                        ),
                    )),
            )
            .when(port_invalid, |section| {
                section.child(
                    self.render_connection_hint_with_color(
                        self.i18n
                            .t("modals.new_connection.remote_desktop_invalid_port"),
                        self.tokens.ui.error,
                    ),
                )
            })
            .into_any_element();
        let authentication = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .when(
                protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
                |section| {
                    section.child(self.render_connection_field(
                        self.i18n.t("modals.new_connection.remote_desktop_username"),
                        &username,
                        "Administrator".to_string(),
                        NewConnectionField::Username,
                        false,
                        cx,
                    ))
                },
            )
            .child(self.render_connection_secret_field(
                self.i18n.t("ssh.form.password"),
                if keeps_saved_password {
                    self.i18n
                        .t("modals.new_connection.remote_desktop_password_keep_placeholder")
                } else if protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp {
                    self.i18n
                        .t("modals.new_connection.remote_desktop_password_placeholder")
                } else {
                    self.i18n.t("ssh.form.password")
                },
                NewConnectionField::Password,
                cx,
            ))
            .child(self.render_connection_checkbox(
                self.i18n.t("ssh.form.save_password"),
                save_password,
                |form| form.save_password = !form.save_password,
                cx,
            ))
            .into_any_element();

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_form_section(ConnectionFormSection::Basic, basic, cx))
            .child(self.render_connection_form_section(
                ConnectionFormSection::Authentication,
                authentication,
                cx,
            ))
            .child({
                let gateway = self.render_remote_desktop_ssh_gateway_select(
                    ssh_gateway_connection_id.as_deref(),
                    cx,
                );
                self.render_connection_form_section(
                    ConnectionFormSection::RemoteGateway,
                    gateway,
                    cx,
                )
            })
            .when(
                protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc,
                |section| {
                    let preferences = self.render_vnc_connection_preferences(cx);
                    section.child(self.render_connection_form_section(
                        ConnectionFormSection::VncPreferences,
                        preferences,
                        cx,
                    ))
                },
            )
            .child({
                let features = self.render_remote_desktop_features(protocol, &capabilities, cx);
                self.render_connection_form_section(
                    ConnectionFormSection::RemoteFeatures,
                    features,
                    cx,
                )
            })
            .into_any_element()
    }

    fn render_vnc_connection_preferences(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_security_policy",
                "modals.new_connection.vnc_security_policy_hint",
                VNC_SECURITY_PREFERENCES,
                cx,
            ))
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_session_mode",
                "modals.new_connection.vnc_session_mode_hint",
                VNC_SESSION_MODE_PREFERENCES,
                cx,
            ))
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_image_quality",
                "modals.new_connection.vnc_image_quality_hint",
                VNC_IMAGE_QUALITY_PREFERENCES,
                cx,
            ))
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_compression",
                "modals.new_connection.vnc_compression_hint",
                VNC_COMPRESSION_PREFERENCES,
                cx,
            ))
            .into_any_element()
    }

    fn render_vnc_preference_group(
        &self,
        title_key: &'static str,
        hint_key: &'static str,
        preferences: &'static [(RemoteDesktopVncPreference, &str)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.remote_desktop_session_options.vnc)
            .unwrap_or_default();
        let options = preferences
            .iter()
            .enumerate()
            .map(|(index, (preference, label_key))| {
                let preference = *preference;
                segmented_tab(
                    &self.tokens,
                    self.i18n.t(label_key),
                    remote_desktop_vnc_preference_selected(&current, preference),
                )
                .id(SharedString::from(format!(
                    "vnc-preference-{title_key}-{index}"
                )))
                .whitespace_normal()
                .text_align(gpui::TextAlign::Center)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                apply_remote_desktop_vnc_preference(
                                    &mut form.remote_desktop_session_options.vnc,
                                    preference,
                                );
                            }
                        });
                        cx.notify();
                    }),
                )
            });

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one))
            .child(form_field(
                &self.tokens,
                self.i18n.t(title_key),
                segmented_tabs(&self.tokens).children(options),
            ))
            .child(self.render_connection_hint(self.i18n.t(hint_key)))
            .into_any_element()
    }

    fn render_remote_desktop_features(
        &self,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol,
        capabilities: &oxideterm_remote_desktop::RemoteDesktopProviderCapabilities,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(self.render_remote_desktop_feature_group(
                "modals.new_connection.remote_desktop_clipboard_group",
                REMOTE_DESKTOP_CLIPBOARD_FEATURES,
                capabilities,
                cx,
            ))
            .child(self.render_remote_desktop_feature_group(
                "modals.new_connection.remote_desktop_audio_group",
                REMOTE_DESKTOP_AUDIO_FEATURES,
                capabilities,
                cx,
            ))
            .child(self.render_remote_desktop_feature_group(
                "modals.new_connection.remote_desktop_display_group",
                REMOTE_DESKTOP_DISPLAY_FEATURES,
                capabilities,
                cx,
            ))
            .when(
                protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
                |features| {
                    features.child(self.render_remote_desktop_feature_group(
                        "modals.new_connection.remote_desktop_compatibility_group",
                        RDP_COMPATIBILITY_FEATURES,
                        capabilities,
                        cx,
                    ))
                },
            )
            .into_any_element()
    }

    fn render_remote_desktop_feature_group(
        &self,
        title_key: &str,
        features: &[(RemoteDesktopSessionFeature, &str, &str)],
        capabilities: &oxideterm_remote_desktop::RemoteDesktopProviderCapabilities,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let feature_grid = div()
            .grid()
            .grid_cols(remote_desktop_feature_columns(features.len()))
            .gap(px(self.tokens.spacing.two))
            .children(features.iter().map(|(feature, label_key, hint_key)| {
                self.render_remote_desktop_feature_row(
                    self.i18n.t(label_key),
                    self.i18n.t(hint_key),
                    remote_desktop_feature_supported(capabilities, *feature),
                    *feature,
                    cx,
                )
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(title_key)),
            )
            .child(feature_grid)
            .into_any_element()
    }

    fn render_remote_desktop_feature_row(
        &self,
        label: String,
        hint: String,
        supported: bool,
        feature: RemoteDesktopSessionFeature,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| {
                supported
                    && remote_desktop_feature_selected(
                        &form.remote_desktop_session_options,
                        feature,
                    )
            });
        let hint = if supported {
            hint
        } else {
            format!(
                "{hint} · {}",
                self.i18n
                    .t("modals.new_connection.remote_desktop_feature_unsupported")
            )
        };

        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one))
            .child(
                checkbox_with(
                    &self.tokens,
                    label,
                    selected,
                    CheckboxOptions {
                        disabled: !supported,
                        ..CheckboxOptions::default()
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if supported {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    // Session feature choices are immutable once the helper starts.
                                    toggle_remote_desktop_feature(
                                        &mut form.remote_desktop_session_options,
                                        feature,
                                    );
                                }
                            });
                        }
                        this.close_new_connection_select(cx);
                        cx.notify();
                    }),
                ),
            )
            .child(
                div()
                    .min_w_0()
                    .pl(px(
                        self.tokens.metrics.ui_checkbox_size + self.tokens.spacing.two
                    ))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(if supported {
                        self.tokens.ui.text_muted
                    } else {
                        self.tokens.ui.warning
                    }))
                    .child(hint),
            )
            .into_any_element()
    }

    pub(super) fn render_telnet_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((host, port, profile_name)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.host.clone(),
                    form.port.clone(),
                    form.telnet_profile_name.clone(),
                )
            })
        else {
            return div().into_any_element();
        };
        let telnet_port_invalid = !port.trim().is_empty() && port.trim().parse::<u16>().is_err();
        let basic = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                self.render_connection_field(
                    self.i18n.t("modals.new_connection.telnet_profile_name"),
                    &profile_name,
                    self.i18n
                        .t("modals.new_connection.telnet_profile_name_placeholder"),
                    NewConnectionField::TelnetProfileName,
                    false,
                    cx,
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(div().flex_1().child(self.render_connection_field(
                        self.i18n.t("modals.new_connection.telnet_host"),
                        &host,
                        self.i18n.t("modals.new_connection.telnet_host_placeholder"),
                        NewConnectionField::Host,
                        false,
                        cx,
                    )))
                    .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.telnet_port"),
                            &port,
                            TELNET_DEFAULT_PORT_TEXT.to_string(),
                            NewConnectionField::Port,
                            false,
                            cx,
                        ),
                    )),
            )
            .when(telnet_port_invalid, |section| {
                section.child(self.render_connection_hint_with_color(
                    self.i18n.t("modals.new_connection.telnet_invalid_port"),
                    self.tokens.ui.error,
                ))
            })
            .into_any_element();
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_form_section(ConnectionFormSection::Basic, basic, cx))
            .child(self.render_connection_form_section(
                ConnectionFormSection::Terminal,
                self.render_connection_terminal_options(cx),
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn refresh_serial_ports(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.serial_ports_loading = true;
                form.error = None;
            }
        });
        cx.notify();

        let result = oxideterm_terminal::serial_list_ports();
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.serial_ports_loading = false;
                match result {
                    Ok(ports) => {
                        if form.serial_port_path.trim().is_empty()
                            && let Some(first_port) = ports.first()
                        {
                            form.serial_port_path = first_port.port_path.clone();
                        }
                        form.serial_ports = ports;
                    }
                    Err(error) => {
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n
                                .t("modals.new_connection.serial_load_ports_failed")
                        ));
                    }
                }
            }
        });
        cx.notify();
    }

    pub(super) fn render_serial_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((ports, baud_rate, data_bits, stop_bits, parity, flow_control, profile_name)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.serial_ports.clone(),
                    form.serial_baud_rate.clone(),
                    form.serial_data_bits,
                    form.serial_stop_bits,
                    form.serial_parity,
                    form.serial_flow_control,
                    form.serial_profile_name.clone(),
                )
            })
        else {
            return div().into_any_element();
        };
        let serial_baud_rate_invalid = !baud_rate.trim().is_empty()
            && !baud_rate.trim().parse::<u32>().is_ok_and(|baud| baud > 0);
        let parameters = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_serial_port_field(&ports, cx))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.spacing.two))
                            .child(self.render_connection_field(
                                self.i18n.t("modals.new_connection.serial_baud_rate"),
                                &baud_rate,
                                "115200".to_string(),
                                NewConnectionField::SerialBaudRate,
                                false,
                                cx,
                            ))
                            .when(serial_baud_rate_invalid, |section| {
                                section.child(
                                    self.render_connection_hint_with_color(
                                        self.i18n
                                            .t("modals.new_connection.serial_invalid_baud_rate"),
                                        self.tokens.ui.error,
                                    ),
                                )
                            }),
                    )
                    .child(self.render_serial_u8_select(
                        self.i18n.t("modals.new_connection.serial_data_bits"),
                        NewConnectionSelect::SerialDataBits,
                        &[(5, "5"), (6, "6"), (7, "7"), (8, "8")],
                        data_bits,
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(self.render_serial_u8_select(
                        self.i18n.t("modals.new_connection.serial_stop_bits"),
                        NewConnectionSelect::SerialStopBits,
                        &[(1, "1"), (2, "2")],
                        stop_bits,
                        cx,
                    ))
                    .child(self.render_serial_parity_select(parity, cx))
                    .child(self.render_serial_flow_select(flow_control, cx)),
            )
            .into_any_element();
        let basic = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                self.render_connection_field(
                    self.i18n.t("modals.new_connection.serial_profile_name"),
                    &profile_name,
                    self.i18n
                        .t("modals.new_connection.serial_profile_name_placeholder"),
                    NewConnectionField::SerialProfileName,
                    false,
                    cx,
                ),
            )
            .into_any_element();
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_form_section(ConnectionFormSection::Basic, basic, cx))
            .child(self.render_connection_form_section(
                ConnectionFormSection::SerialParameters,
                parameters,
                cx,
            ))
            .into_any_element()
    }

    fn render_serial_port_field(
        &self,
        ports: &[oxideterm_terminal::SerialPortInfo],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return div().into_any_element();
        };
        let loading = form.serial_ports_loading;
        let selected_port = form.serial_port_path.clone();
        let port_selector = if ports.is_empty() {
            self.render_connection_hint(if loading {
                self.i18n.t("modals.new_connection.serial_loading_ports")
            } else {
                self.i18n.t("modals.new_connection.serial_no_ports")
            })
        } else {
            let selected_label = ports
                .iter()
                .find(|port| port.port_path == selected_port)
                .map(serial_port_display_label)
                .unwrap_or_else(|| {
                    if selected_port.trim().is_empty() {
                        self.i18n
                            .t("modals.new_connection.serial_select_detected_port")
                    } else {
                        selected_port.clone()
                    }
                });
            // Tauri renders detected serial ports as a Radix Select below the
            // editable path input; keep manual entry and detected-choice paths separate.
            self.render_new_connection_select_control(
                NewConnectionSelect::SerialPort,
                selected_label,
                selected_port.trim().is_empty(),
                false,
                cx,
            )
        };

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(format!(
                                "{} *",
                                self.i18n.t("modals.new_connection.serial_port")
                            )),
                    )
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("modals.new_connection.serial_refresh_ports"),
                        Some(if loading {
                            self.render_loading_icon(
                                "serial-ports-loading",
                                14.0,
                                rgb(self.tokens.ui.text),
                            )
                        } else {
                            Self::render_lucide_icon(
                                LucideIcon::RefreshCw,
                                14.0,
                                rgb(self.tokens.ui.text),
                            )
                        }),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                disabled: loading,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.refresh_serial_ports(cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .child(self.render_connection_input(
                &selected_port,
                self.i18n.t("modals.new_connection.serial_port_placeholder"),
                NewConnectionField::SerialPortPath,
                false,
                cx,
            ))
            .child(port_selector)
            .into_any_element()
    }

    fn render_new_connection_select_control(
        &self,
        select_id: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger = self
            .new_connection_select_trigger(select_id, value, placeholder, disabled, cx)
            .when(!disabled, |trigger| {
                trigger.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.field_focused = false;
                                form.selected_field = None;
                            }
                        });
                        this.ime_marked_text = None;
                        this.open_new_connection_select_from_pointer(select_id, cx);
                        window.focus(&this.focus_handle, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
            });

        self.track_new_connection_select_anchor(select_id, trigger, cx)
    }

    fn render_connection_checkbox_with_help(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        toggle: fn(&mut NewConnectionForm),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Security-sensitive SSH toggles keep their warning available without
        // adding permanent helper copy to an already dense advanced section.
        div()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(self.render_connection_checkbox(self.i18n.t(label_key), checked, toggle, cx))
            .child(
                div()
                    .id(trigger_id)
                    .size(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Info,
                        14.0,
                        rgb(self.tokens.ui.warning),
                    ))
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                            this.queue_workspace_tooltip(
                                tooltip_id,
                                this.i18n.t(hint_key),
                                f32::from(event.position.x) + 12.0,
                                f32::from(event.position.y) + 16.0,
                                cx,
                            );
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.clear_workspace_tooltip(tooltip_id, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            // Tooltip content is portalled, so the trigger clears ownership.
                            this.clear_workspace_tooltip(tooltip_id, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    pub(super) fn render_connection_terminal_options(&self, cx: &mut Context<Self>) -> AnyElement {
        // Saved host controls are optional overrides so application defaults
        // continue to govern legacy records and temporary local terminals.
        let Some((terminal, dedicated_new_terminal_connection)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.terminal.clone(),
                    form.dedicated_new_terminal_connection,
                )
            })
        else {
            return div().into_any_element();
        };
        let application_defaults = &self.settings_store.settings().terminal;
        let default_encoding = terminal_encoding_label(application_defaults.terminal_encoding);
        let default_backspace =
            terminal_backspace_sequence_label(application_defaults.backspace_sequence);
        let default_delete = terminal_delete_sequence_label(application_defaults.delete_sequence);
        let default_scheme = application_defaults
            .active_custom_semantic_scheme()
            .map(|scheme| scheme.name.clone())
            .unwrap_or_else(|| match application_defaults.semantic_scheme {
                oxideterm_settings::TerminalSemanticScheme::Balanced => self
                    .i18n
                    .t("settings_view.terminal.highlight_rules.semantic_scheme_balanced"),
                oxideterm_settings::TerminalSemanticScheme::Conservative => self
                    .i18n
                    .t("settings_view.terminal.highlight_rules.semantic_scheme_conservative"),
            });
        let inherited_label = |value: &str| {
            self.i18n
                .t("ssh.form.terminal_use_application_default")
                .replace("{{value}}", value)
        };
        let encoding_label = terminal
            .encoding
            .map(connection_terminal_encoding_label)
            .map(str::to_string)
            .unwrap_or_else(|| inherited_label(&default_encoding));
        let backspace_label = terminal
            .backspace_sequence
            .map(connection_terminal_backspace_sequence_label)
            .map(str::to_string)
            .unwrap_or_else(|| inherited_label(default_backspace));
        let delete_label = terminal
            .delete_sequence
            .map(connection_terminal_delete_sequence_label)
            .map(str::to_string)
            .unwrap_or_else(|| inherited_label(default_delete));
        let scheme_label = terminal
            .semantic_scheme
            .as_deref()
            .map(|id| match id {
                "balanced" => self
                    .i18n
                    .t("settings_view.terminal.highlight_rules.semantic_scheme_balanced"),
                "conservative" => self
                    .i18n
                    .t("settings_view.terminal.highlight_rules.semantic_scheme_conservative"),
                custom_id => application_defaults
                    .custom_semantic_schemes
                    .iter()
                    .find(|scheme| scheme.id == custom_id)
                    .map(|scheme| scheme.name.clone())
                    .unwrap_or_else(|| custom_id.to_string()),
            })
            .unwrap_or_else(|| inherited_label(&default_scheme));
        let default_highlight_rule_set = application_defaults
            .default_highlight_rule_set_name()
            .map(str::to_string)
            .unwrap_or_else(|| {
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_global_base")
            });
        let highlight_rule_set_label = terminal
            .highlight_rule_set
            .as_deref()
            .and_then(|id| application_defaults.highlight_rule_set(id))
            .map(|rule_set| rule_set.name.clone())
            .unwrap_or_else(|| inherited_label(&default_highlight_rule_set));

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(form_field(
                                &self.tokens,
                                self.i18n.t("settings_view.terminal.encoding"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::TerminalEncoding,
                                    encoding_label,
                                    false,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(form_field(
                                &self.tokens,
                                self.i18n.t("settings_view.terminal.backspace_sequence"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::TerminalBackspaceSequence,
                                    backspace_label,
                                    false,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(form_field(
                                &self.tokens,
                                self.i18n.t("settings_view.terminal.delete_sequence"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::TerminalDeleteSequence,
                                    delete_label,
                                    false,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(form_field(
                                &self.tokens,
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.semantic_scheme"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::TerminalSemanticScheme,
                                    scheme_label,
                                    false,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_TERMINAL_CONTROL_MIN_WIDTH))
                            .child(form_field(
                                &self.tokens,
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.rule_set"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::TerminalHighlightRuleSet,
                                    highlight_rule_set_label,
                                    false,
                                    false,
                                    cx,
                                ),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(self.render_connection_checkbox(
                        self.i18n.t("ssh.form.dedicated_new_terminal_connection"),
                        dedicated_new_terminal_connection,
                        |form| {
                            form.dedicated_new_terminal_connection =
                                !form.dedicated_new_terminal_connection;
                        },
                        cx,
                    ))
                    .child(
                        div()
                            .pl(px(self.tokens.spacing.three))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("ssh.form.dedicated_new_terminal_connection_hint"),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_connection_ssh_options(
        &self,
        agent_forwarding: bool,
        legacy_ssh_compatibility: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((connect_timeout_seconds, x11_forwarding)) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| (form.connect_timeout_seconds, form.x11_forwarding))
        else {
            return div().into_any_element();
        };
        let mut timeout_options = SSH_CONNECT_TIMEOUT_OPTIONS_SECONDS.to_vec();
        if !timeout_options.contains(&connect_timeout_seconds) {
            timeout_options.push(connect_timeout_seconds);
            timeout_options.sort_unstable();
        }
        let timeout_tabs = timeout_options.into_iter().map(|seconds| {
            segmented_tab(
                &self.tokens,
                self.i18n
                    .t("ssh.form.connect_timeout_value")
                    .replace("{{seconds}}", &seconds.to_string()),
                connect_timeout_seconds == seconds,
            )
            .id(SharedString::from(format!("ssh-connect-timeout-{seconds}")))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.connect_timeout_seconds = seconds;
                        }
                    });
                    cx.notify();
                }),
            )
        });
        let x11_mode_options = [
            (
                ConnectionX11ForwardingMode::Untrusted,
                "ssh.form.x11_mode_untrusted",
            ),
            (
                ConnectionX11ForwardingMode::Trusted,
                "ssh.form.x11_mode_trusted",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (mode, label_key))| {
            segmented_tab(
                &self.tokens,
                self.i18n.t(label_key),
                x11_forwarding.mode == mode,
            )
            .id(SharedString::from(format!("x11-mode-{index}")))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.x11_forwarding.mode = mode;
                        }
                    });
                    cx.notify();
                }),
            )
        })
        .collect::<Vec<_>>();
        let x11_timeout_options = [
            (300u32, "ssh.form.x11_timeout_5_minutes"),
            (1_200u32, "ssh.form.x11_timeout_20_minutes"),
            (3_600u32, "ssh.form.x11_timeout_1_hour"),
            (0u32, "ssh.form.x11_timeout_none"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (seconds, label_key))| {
            segmented_tab(
                &self.tokens,
                self.i18n.t(label_key),
                x11_forwarding.untrusted_timeout_seconds == seconds,
            )
            .id(SharedString::from(format!("x11-timeout-{index}")))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.x11_forwarding.untrusted_timeout_seconds = seconds;
                        }
                    });
                    cx.notify();
                }),
            )
        })
        .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(form_field(
                &self.tokens,
                self.i18n.t("ssh.form.connect_timeout"),
                segmented_tabs(&self.tokens).children(timeout_tabs),
            ))
            .child(self.render_connection_hint(self.i18n.t("ssh.form.connect_timeout_hint")))
            .child(self.render_connection_checkbox_with_help(
                "new-connection-agent-forwarding-help",
                "new-connection-agent-forwarding",
                "ssh.form.agent_forwarding",
                "ssh.form.agent_forwarding_hint",
                agent_forwarding,
                |form| form.agent_forwarding = !form.agent_forwarding,
                cx,
            ))
            .child(self.render_connection_checkbox(
                self.i18n.t("ssh.form.x11_forwarding"),
                x11_forwarding.enabled,
                |form| form.x11_forwarding.enabled = !form.x11_forwarding.enabled,
                cx,
            ))
            .child(self.render_connection_hint(self.i18n.t("ssh.form.x11_forwarding_hint")))
            .when(x11_forwarding.enabled, |content| {
                content
                    .child(form_field(
                        &self.tokens,
                        self.i18n.t("ssh.form.x11_mode"),
                        segmented_tabs(&self.tokens).children(x11_mode_options),
                    ))
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.x11_mode_hint")))
            })
            .when(
                x11_forwarding.enabled
                    && x11_forwarding.mode == ConnectionX11ForwardingMode::Untrusted,
                |content| {
                    content
                        .child(form_field(
                            &self.tokens,
                            self.i18n.t("ssh.form.x11_timeout"),
                            segmented_tabs(&self.tokens).children(x11_timeout_options),
                        ))
                        .child(
                            self.render_connection_hint(self.i18n.t("ssh.form.x11_timeout_hint")),
                        )
                },
            )
            .child(self.render_connection_checkbox_with_help(
                "new-connection-legacy-ssh-compatibility-help",
                "new-connection-legacy-ssh-compatibility",
                "ssh.form.legacy_ssh_compatibility",
                "ssh.form.legacy_ssh_compatibility_hint",
                legacy_ssh_compatibility,
                |form| form.legacy_ssh_compatibility = !form.legacy_ssh_compatibility,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn set_new_connection_terminal_encoding(
        &mut self,
        encoding: Option<ConnectionTerminalEncoding>,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.encoding = encoding;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_terminal_backspace_sequence(
        &mut self,
        sequence: Option<ConnectionTerminalBackspaceSequence>,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.backspace_sequence = sequence;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_terminal_delete_sequence(
        &mut self,
        sequence: Option<ConnectionTerminalDeleteSequence>,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.delete_sequence = sequence;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_terminal_semantic_scheme(
        &mut self,
        scheme_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.semantic_scheme = scheme_id;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_terminal_highlight_rule_set(
        &mut self,
        rule_set_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.highlight_rule_set = rule_set_id;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    fn render_serial_u8_select(
        &self,
        label: String,
        select_id: NewConnectionSelect,
        choices: &[(u8, &'static str)],
        selected: u8,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = choices
            .iter()
            .find(|(value, _)| *value == selected)
            .map(|(_, option_label)| (*option_label).to_string())
            .unwrap_or_else(|| selected.to_string());
        // Tauri serial numeric choices are Select controls, not segmented tabs.
        form_field(
            &self.tokens,
            label,
            self.render_new_connection_select_control(select_id, selected_label, false, false, cx),
        )
        .into_any_element()
    }

    fn render_serial_parity_select(
        &self,
        selected: oxideterm_terminal::SerialParity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.serial_parity"),
            self.render_new_connection_select_control(
                NewConnectionSelect::SerialParity,
                self.serial_parity_label(selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_serial_flow_select(
        &self,
        selected: oxideterm_terminal::SerialFlowControl,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.serial_flow_control"),
            self.render_new_connection_select_control(
                NewConnectionSelect::SerialFlowControl,
                self.serial_flow_control_label(selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn upstream_proxy_policy_label(&self, policy: NewConnectionUpstreamProxyPolicy) -> String {
        let key = match policy {
            NewConnectionUpstreamProxyPolicy::UseGlobal => "modals.upstream_proxy.use_global",
            NewConnectionUpstreamProxyPolicy::Direct => "modals.upstream_proxy.direct",
            NewConnectionUpstreamProxyPolicy::Custom => "modals.upstream_proxy.custom",
        };
        self.i18n.t(key)
    }

    fn upstream_proxy_protocol_label(&self, protocol: SavedUpstreamProxyProtocol) -> String {
        let key = match protocol {
            SavedUpstreamProxyProtocol::Socks5 => "settings_view.network.protocol_socks5",
            SavedUpstreamProxyProtocol::HttpConnect => {
                "settings_view.network.protocol_http_connect"
            }
        };
        self.i18n.t(key)
    }

    fn upstream_proxy_auth_label(&self, auth: NewConnectionUpstreamProxyAuth) -> String {
        let key = match auth {
            NewConnectionUpstreamProxyAuth::None => "settings_view.network.auth_none",
            NewConnectionUpstreamProxyAuth::Password => "settings_view.network.auth_password",
        };
        self.i18n.t(key)
    }

    pub(super) fn render_upstream_proxy_policy_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((policy, protocol, host, port, no_proxy, remote_dns, auth, username)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.upstream_proxy_policy,
                    form.upstream_proxy_protocol,
                    form.upstream_proxy_host.clone(),
                    form.upstream_proxy_port.clone(),
                    form.upstream_proxy_no_proxy.clone(),
                    form.upstream_proxy_remote_dns,
                    form.upstream_proxy_auth,
                    form.upstream_proxy_username.clone(),
                )
            })
        else {
            return div().into_any_element();
        };
        let custom = policy == NewConnectionUpstreamProxyPolicy::Custom;
        div()
            .flex()
            .flex_col()
            .gap_4()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .pt_4()
            .child(form_field(
                &self.tokens,
                self.i18n.t("modals.upstream_proxy.policy"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::UpstreamProxyPolicy,
                    self.upstream_proxy_policy_label(policy),
                    false,
                    false,
                    cx,
                ),
            ))
            .child(self.render_connection_hint(self.i18n.t("modals.upstream_proxy.policy_hint")))
            .when(custom, |content| {
                content
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(div().flex_1().child(form_field(
                                &self.tokens,
                                self.i18n.t("settings_view.network.protocol"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::UpstreamProxyProtocol,
                                    self.upstream_proxy_protocol_label(protocol),
                                    false,
                                    false,
                                    cx,
                                ),
                            )))
                            .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                                self.render_connection_field(
                                    self.i18n.t("settings_view.network.port"),
                                    &port,
                                    "1080".to_string(),
                                    NewConnectionField::UpstreamProxyPort,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(self.render_connection_field(
                        self.i18n.t("settings_view.network.host"),
                        &host,
                        "127.0.0.1".to_string(),
                        NewConnectionField::UpstreamProxyHost,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_field(
                        self.i18n.t("settings_view.network.no_proxy"),
                        &no_proxy,
                        "localhost,127.0.0.1,*.internal".to_string(),
                        NewConnectionField::UpstreamProxyNoProxy,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_checkbox(
                        self.i18n.t("settings_view.network.remote_dns"),
                        remote_dns,
                        |form| form.upstream_proxy_remote_dns = !form.upstream_proxy_remote_dns,
                        cx,
                    ))
                    .child(form_field(
                        &self.tokens,
                        self.i18n.t("settings_view.network.auth"),
                        self.render_new_connection_select_control(
                            NewConnectionSelect::UpstreamProxyAuth,
                            self.upstream_proxy_auth_label(auth),
                            false,
                            false,
                            cx,
                        ),
                    ))
                    .when(
                        auth == NewConnectionUpstreamProxyAuth::Password,
                        |content| {
                            content
                                .child(self.render_connection_field(
                                    self.i18n.t("settings_view.network.username"),
                                    &username,
                                    String::new(),
                                    NewConnectionField::UpstreamProxyUsername,
                                    false,
                                    cx,
                                ))
                                .child(self.render_connection_secret_field(
                                    self.i18n.t("settings_view.network.password"),
                                    String::new(),
                                    NewConnectionField::UpstreamProxyPassword,
                                    cx,
                                ))
                                .child(self.render_connection_hint(
                                    self.i18n.t("settings_view.network.password_hint"),
                                ))
                        },
                    )
            })
            .into_any_element()
    }

    pub(super) fn serial_parity_label(&self, parity: oxideterm_terminal::SerialParity) -> String {
        match parity {
            oxideterm_terminal::SerialParity::None => {
                self.i18n.t("modals.new_connection.serial_parity_none")
            }
            oxideterm_terminal::SerialParity::Odd => {
                self.i18n.t("modals.new_connection.serial_parity_odd")
            }
            oxideterm_terminal::SerialParity::Even => {
                self.i18n.t("modals.new_connection.serial_parity_even")
            }
        }
    }

    pub(super) fn serial_flow_control_label(
        &self,
        flow: oxideterm_terminal::SerialFlowControl,
    ) -> String {
        match flow {
            oxideterm_terminal::SerialFlowControl::None => {
                self.i18n.t("modals.new_connection.serial_flow_none")
            }
            oxideterm_terminal::SerialFlowControl::Software => {
                self.i18n.t("modals.new_connection.serial_flow_software")
            }
            oxideterm_terminal::SerialFlowControl::Hardware => {
                self.i18n.t("modals.new_connection.serial_flow_hardware")
            }
        }
    }

    pub(super) fn set_new_connection_serial_port(
        &mut self,
        port_path: String,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.serial_port_path = port_path;
                form.focused_field = NewConnectionField::SerialPortPath;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_u8(
        &mut self,
        select_id: NewConnectionSelect,
        value: u8,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                match select_id {
                    NewConnectionSelect::SerialDataBits => form.serial_data_bits = value,
                    NewConnectionSelect::SerialStopBits => form.serial_stop_bits = value,
                    _ => return,
                }
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_parity(
        &mut self,
        parity: oxideterm_terminal::SerialParity,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.serial_parity = parity;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_flow_control(
        &mut self,
        flow: oxideterm_terminal::SerialFlowControl,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.serial_flow_control = flow;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_policy(
        &mut self,
        policy: NewConnectionUpstreamProxyPolicy,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.upstream_proxy_policy = policy;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_protocol(
        &mut self,
        protocol: SavedUpstreamProxyProtocol,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.upstream_proxy_protocol = protocol;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_auth(
        &mut self,
        auth: NewConnectionUpstreamProxyAuth,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                if auth == NewConnectionUpstreamProxyAuth::None {
                    // Hidden password fields should not retain a draft secret after
                    // switching the custom proxy back to unauthenticated mode.
                    form.upstream_proxy_password.clear();
                    form.upstream_proxy_password_keychain_id = None;
                }
                form.upstream_proxy_auth = auth;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn render_connection_checkbox(
        &self,
        label: String,
        checked: bool,
        toggle: fn(&mut NewConnectionForm),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, label, checked)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            toggle(form);
                        }
                    });
                    this.close_new_connection_select(cx);
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_connection_button(
        &self,
        label: String,
        primary: bool,
        action: ConnectionButtonAction,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // NewConnectionModal footer uses shadcn Button variants. Route native
        // footer buttons through the shared toolbar primitive while keeping the
        // existing form action dispatch unchanged.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if primary {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Secondary
                    },
                    disabled,
                    ..ButtonOptions::default()
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| match action {
                ConnectionButtonAction::Cancel => {
                    this.close_new_connection_form(window, cx);
                }
                ConnectionButtonAction::Test => {
                    this.start_new_connection_flow(SshConnectionIntent::Test, window, cx);
                }
                ConnectionButtonAction::Connect => {
                    this.submit_new_connection_form_with_action(
                        NewConnectionSubmitAction::Connect,
                        window,
                        cx,
                    );
                }
                ConnectionButtonAction::Save => {
                    this.submit_new_connection_form_with_action(
                        NewConnectionSubmitAction::Save,
                        window,
                        cx,
                    );
                }
                ConnectionButtonAction::SaveAndConnect => {
                    this.submit_new_connection_form_with_action(
                        NewConnectionSubmitAction::SaveAndConnect,
                        window,
                        cx,
                    );
                }
            }),
        )
        .into_any_element()
    }
}

pub(super) fn serial_port_display_label(port: &oxideterm_terminal::SerialPortInfo) -> String {
    if port.display_name.trim().is_empty() {
        port.port_path.clone()
    } else {
        port.display_name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_selection_offset_matches_fixed_row_stride() {
        assert_eq!(
            new_connection_transport_vertical_offset(
                NewConnectionTransport::Ssh,
                NewConnectionTransport::Vnc,
            ),
            -200.0,
        );
        assert_eq!(
            new_connection_transport_vertical_offset(
                NewConnectionTransport::Vnc,
                NewConnectionTransport::Telnet,
            ),
            120.0,
        );
    }

    #[test]
    fn windows_only_transport_follows_shared_transport_order() {
        assert_eq!(
            new_connection_transport_index(NewConnectionTransport::WslGraphics),
            6,
        );
        assert_eq!(
            new_connection_transport_index(NewConnectionTransport::LocalTerminal),
            if cfg!(target_os = "windows") { 7 } else { 6 },
        );
    }

    #[test]
    fn terminal_selects_map_to_their_tracked_overlay_anchors() {
        for (select, anchor_id) in [
            (
                NewConnectionSelect::TerminalEncoding,
                SelectAnchorId::NewConnectionTerminalEncoding,
            ),
            (
                NewConnectionSelect::TerminalBackspaceSequence,
                SelectAnchorId::NewConnectionTerminalBackspaceSequence,
            ),
            (
                NewConnectionSelect::TerminalDeleteSequence,
                SelectAnchorId::NewConnectionTerminalDeleteSequence,
            ),
        ] {
            assert_eq!(
                WorkspaceApp::new_connection_select_anchor_id(select),
                anchor_id
            );
            assert!(anchor_id.is_new_connection_select_trigger());
        }
    }

    #[test]
    fn form_sections_start_open_until_the_user_overrides_them() {
        let mut form = NewConnectionForm::default();
        for section in [
            ConnectionFormSection::Basic,
            ConnectionFormSection::Authentication,
            ConnectionFormSection::Route,
            ConnectionFormSection::SshOptions,
            ConnectionFormSection::Terminal,
            ConnectionFormSection::Appearance,
            ConnectionFormSection::RemoteGateway,
            ConnectionFormSection::VncPreferences,
            ConnectionFormSection::RemoteFeatures,
            ConnectionFormSection::SerialParameters,
            ConnectionFormSection::MoshOptions,
            ConnectionFormSection::LocalShell,
        ] {
            assert!(connection_form_section_expanded_for_form(&form, section));
        }

        form.route_section_expanded = Some(false);
        assert!(!connection_form_section_expanded_for_form(
            &form,
            ConnectionFormSection::Route,
        ));
        form.route_section_expanded = Some(true);
        assert!(connection_form_section_expanded_for_form(
            &form,
            ConnectionFormSection::Route,
        ));
        form.local_shell_section_expanded = Some(false);
        assert!(!connection_form_section_expanded_for_form(
            &form,
            ConnectionFormSection::LocalShell,
        ));
        form.authentication_section_expanded = Some(false);
        assert!(!connection_form_section_expanded_for_form(
            &form,
            ConnectionFormSection::Authentication,
        ));
    }

    #[test]
    fn remote_desktop_feature_groups_use_compact_columns() {
        // Single display options stay full-width; larger groups use two columns.
        assert_eq!(remote_desktop_feature_columns(0), 1);
        assert_eq!(remote_desktop_feature_columns(1), 1);
        assert_eq!(remote_desktop_feature_columns(2), 2);
        assert_eq!(remote_desktop_feature_columns(3), 2);
    }

    #[test]
    fn secret_field_render_values_borrow_entity_owned_allocations() {
        let mut form = NewConnectionForm::default();
        form.password = "primary-password".to_string();
        form.passphrase = "primary-passphrase".to_string();
        form.upstream_proxy_password = "proxy-password".to_string();
        let mut jump_form = NewConnectionProxyHop::new();
        jump_form.password = "jump-password".to_string();
        jump_form.passphrase = "jump-passphrase".to_string();
        form.jump_server_form = Some(jump_form);

        let password_pointer = form.password.as_ptr();
        let passphrase_pointer = form.passphrase.as_ptr();
        let proxy_password_pointer = form.upstream_proxy_password.as_ptr();
        let jump_password_pointer = form
            .jump_server_form
            .as_ref()
            .expect("jump form should exist")
            .password
            .as_ptr();
        let jump_passphrase_pointer = form
            .jump_server_form
            .as_ref()
            .expect("jump form should exist")
            .passphrase
            .as_ptr();

        assert_eq!(
            connection_secret_field_value(&form, NewConnectionField::Password)
                .expect("password should be rendered")
                .as_ptr(),
            password_pointer,
        );
        assert_eq!(
            connection_secret_field_value(&form, NewConnectionField::Passphrase)
                .expect("passphrase should be rendered")
                .as_ptr(),
            passphrase_pointer,
        );
        assert_eq!(
            connection_secret_field_value(&form, NewConnectionField::UpstreamProxyPassword)
                .expect("proxy password should be rendered")
                .as_ptr(),
            proxy_password_pointer,
        );
        assert_eq!(
            connection_secret_field_value(&form, NewConnectionField::JumpPassword)
                .expect("jump password should be rendered")
                .as_ptr(),
            jump_password_pointer,
        );
        assert_eq!(
            connection_secret_field_value(&form, NewConnectionField::JumpPassphrase)
                .expect("jump passphrase should be rendered")
                .as_ptr(),
            jump_passphrase_pointer,
        );
    }

    #[test]
    fn form_view_sources_do_not_clone_secret_drafts_for_rendering() {
        let sources = [
            include_str!("form_modal.rs"),
            include_str!("field_controls.rs"),
            include_str!("proxy_chain_view.rs"),
        ];
        let forbidden_patterns = [
            ["form.password", ".clone()"].concat(),
            ["form.passphrase", ".clone()"].concat(),
            ["form.upstream_proxy_password", ".clone()"].concat(),
            ["hop.password", ".clone()"].concat(),
            ["hop.passphrase", ".clone()"].concat(),
        ];

        for source in sources {
            for forbidden_pattern in &forbidden_patterns {
                assert!(
                    !source.contains(forbidden_pattern),
                    "form rendering must borrow secret drafts instead of matching {forbidden_pattern}",
                );
            }
        }
    }
}
