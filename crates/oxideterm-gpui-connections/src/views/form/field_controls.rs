use super::*;
use crate::form_model::{
    ConnectionTerminalBellPolicy, ConnectionTerminalSessionLogPolicy, SshChannelStrategy,
};
use gpui::{App, CursorStyle, InteractiveElement};
use oxideterm_remote_desktop::RemoteDesktopRdpNetworkProfile;
use oxideterm_settings_model::parse_rgb24_hex;

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
const RDP_NETWORK_PROFILES: &[(RemoteDesktopRdpNetworkProfile, &str)] = &[
    (
        RemoteDesktopRdpNetworkProfile::Automatic,
        "modals.new_connection.remote_desktop_rdp_network_auto",
    ),
    (
        RemoteDesktopRdpNetworkProfile::Lan,
        "modals.new_connection.remote_desktop_rdp_network_lan",
    ),
    (
        RemoteDesktopRdpNetworkProfile::Broadband,
        "modals.new_connection.remote_desktop_rdp_network_broadband",
    ),
    (
        RemoteDesktopRdpNetworkProfile::LowBandwidth,
        "modals.new_connection.remote_desktop_rdp_network_low_bandwidth",
    ),
];
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

/// Icon shown when a connection has no explicit icon, matching the fallback the
/// session manager uses after the profile is saved.
fn default_icon_for_transport(transport: NewConnectionTransport) -> LucideIcon {
    match transport {
        NewConnectionTransport::Ssh => LucideIcon::Server,
        NewConnectionTransport::StandaloneSftp | NewConnectionTransport::Ftp => {
            LucideIcon::FolderSync
        }
        NewConnectionTransport::Mosh => LucideIcon::Wifi,
        NewConnectionTransport::Telnet | NewConnectionTransport::LocalTerminal => {
            LucideIcon::Terminal
        }
        NewConnectionTransport::Serial => LucideIcon::Radio,
        NewConnectionTransport::Ble => LucideIcon::Bluetooth,
        NewConnectionTransport::Rdp
        | NewConnectionTransport::Vnc
        | NewConnectionTransport::WslGraphics => LucideIcon::Monitor,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AuthSelectorContext {
    Standard,
    StandaloneSftpSecondary,
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
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            Some(&form.standalone_sftp_secondary.password)
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            Some(&form.standalone_sftp_secondary.passphrase)
        }
        NewConnectionField::UpstreamProxyPassword => Some(&form.upstream_proxy_password),
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            Some(&form.standalone_sftp_secondary.upstream_proxy_password)
        }
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

fn toggle_primary_sftp_empty_password(form: &mut NewConnectionForm) {
    form.empty_password = !form.empty_password;
    form.field_focused = false;
    form.saved_password_keychain_id = None;
    form.password_from_store = false;
    form.password_loaded = true;
    form.password.zeroize();
}

fn toggle_secondary_sftp_empty_password(form: &mut NewConnectionForm) {
    let endpoint = &mut form.standalone_sftp_secondary;
    endpoint.empty_password = !endpoint.empty_password;
    endpoint.password_keychain_id = None;
    endpoint.password.zeroize();
    form.field_focused = false;
}

fn toggle_primary_sftp_password_persistence(form: &mut NewConnectionForm) {
    form.save_password = !form.save_password;
}

fn toggle_secondary_sftp_password_persistence(form: &mut NewConnectionForm) {
    form.standalone_sftp_secondary.save_password = !form.standalone_sftp_secondary.save_password;
}

fn toggle_primary_sftp_gssapi_delegation(form: &mut NewConnectionForm) {
    form.gssapi_delegate_credentials = !form.gssapi_delegate_credentials;
}

fn toggle_primary_sftp_gssapi(form: &mut NewConnectionForm) {
    form.gssapi_enabled = !form.gssapi_enabled;
}

fn toggle_secondary_sftp_gssapi(form: &mut NewConnectionForm) {
    let endpoint = &mut form.standalone_sftp_secondary;
    endpoint.gssapi_enabled = !endpoint.gssapi_enabled;
}

fn toggle_secondary_sftp_gssapi_delegation(form: &mut NewConnectionForm) {
    let endpoint = &mut form.standalone_sftp_secondary;
    endpoint.gssapi_delegate_credentials = !endpoint.gssapi_delegate_credentials;
}

impl ConnectionFormView {
    pub(in crate::views) fn new_connection_select_anchor_id(
        select: NewConnectionSelect,
    ) -> SelectAnchorId {
        crate::views::select_anchor_id(select)
    }

    fn new_connection_select_trigger(
        &self,
        select: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &App,
    ) -> Div {
        super::super::select::select_trigger(
            self.flow.state(cx),
            &self.tokens,
            select,
            value,
            placeholder,
            disabled,
        )
    }

    fn track_new_connection_select_anchor(
        &self,
        select: NewConnectionSelect,
        trigger: impl IntoElement,
        cx: &App,
    ) -> AnyElement {
        super::super::select::track_select_anchor(&self.flow, select, trigger, cx)
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
            state.select_focus_origin = Some(BrowserFocusOrigin::Pointer);
        });
    }

    pub(in crate::views) fn close_new_connection_select(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, ConnectionFormState::close_select);
    }

    pub(in crate::views) fn clear_new_connection_select_anchor(&mut self, cx: &App) {
        // Every entry in this store belongs to the moving form viewport, so a
        // scroll can invalidate the entire set without enumerating anchor IDs.
        self.flow.anchors(cx).clear();
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

    pub(super) fn render_kerberos_credentials_status(&self, available: Option<bool>) -> AnyElement {
        let (color, label) = match available {
            Some(true) => (
                self.tokens.ui.success,
                self.i18n.t("ssh.form.kerberos_credentials_available"),
            ),
            Some(false) => (
                self.tokens.ui.warning,
                self.i18n.t("ssh.form.kerberos_credentials_unavailable"),
            ),
            None => (
                self.tokens.ui.text_muted,
                self.i18n.t("ssh.form.kerberos_credentials_checking"),
            ),
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

    pub(in crate::views) fn render_connection_field(
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

    pub(super) fn render_connection_multiline_field(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = ConnectionsInput::Form(field);
        let focused = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.field_focused && form.focused_field == field);
        let Some(frame) = self.frame(target, cx) else {
            return div().into_any_element();
        };
        let marked_range = frame.marked_range;
        let selection = frame.selected_range;
        let caret_visible = frame.caret_visible;
        let showing_placeholder = value.is_empty() && marked_range.is_none();
        let display = if showing_placeholder {
            placeholder
        } else {
            frame
                .notes_with_composition
                .unwrap_or_else(|| value.to_owned())
        };
        let theme = self.tokens.ui;
        let mut textarea = div()
            .w_auto()
            .min_w_full()
            .flex_shrink_0()
            .whitespace_nowrap()
            .min_h(px(CONNECTION_NOTES_MIN_HEIGHT))
            .px(px(self.tokens.metrics.ui_control_padding_x))
            .py(px(CONNECTION_NOTES_VERTICAL_PADDING))
            .flex()
            .flex_col()
            .items_start()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(rgba((theme.bg << 8) | 0x80))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(CONNECTION_NOTES_LINE_HEIGHT))
            .text_color(if showing_placeholder {
                rgb(theme.text_muted)
            } else {
                rgb(theme.text)
            });
        let lines = settings_multiline_line_ranges(&display);
        for (index, (line_range, line_text)) in lines.iter().enumerate() {
            let is_last_line = index + 1 == lines.len();
            let local_marked_range = marked_range.as_ref().and_then(|marked| {
                let start = marked.start.max(line_range.start);
                let end = marked.end.min(line_range.end);
                (start < end).then_some(start - line_range.start..end - line_range.start)
            });
            let (line_selection, line_caret) = if showing_placeholder || marked_range.is_some() {
                (None, None)
            } else {
                settings_multiline_line_selection(selection.as_ref(), line_range)
            };
            let segments = if showing_placeholder {
                div().child(line_text.as_str().to_string())
            } else if let Some(marked_range) = local_marked_range {
                text_input_value_segments_with_marked_range(&self.tokens, line_text, marked_range)
            } else {
                text_input_value_segments(
                    &self.tokens,
                    line_text,
                    false,
                    line_selection,
                    line_caret,
                    caret_visible,
                )
            };
            textarea = textarea.child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(CONNECTION_NOTES_LINE_HEIGHT))
                    .min_h(px(CONNECTION_NOTES_LINE_HEIGHT))
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .flex()
                    .items_center()
                    .when(focused && showing_placeholder && index == 0, |line| {
                        line.child(text_caret(&self.tokens, caret_visible))
                    })
                    .child(segments)
                    .when(
                        focused
                            && is_last_line
                            && !showing_placeholder
                            && selection.is_none()
                            && marked_range.is_none(),
                        |line| line.child(text_caret(&self.tokens, caret_visible)),
                    ),
            );
        }
        form_field(
            &self.tokens,
            label,
            self.settings_multiline_input_viewport(
                target,
                self.finish_connection_input(textarea, field, cx),
                cx,
            ),
        )
    }

    pub(super) fn render_connection_notes_fields(
        &self,
        notes: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Notes are ordinary metadata; the warning keeps credentials in protected fields.
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(self.render_connection_multiline_field(
                self.i18n.t("ssh.form.notes"),
                notes,
                self.i18n.t("ssh.form.notes_placeholder"),
                NewConnectionField::Notes,
                cx,
            ))
            .child(self.render_connection_hint(self.i18n.t("ssh.form.notes_hint")))
            .into_any_element()
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
        let form = self.connection_form_state(cx).form.as_ref()?;
        let use_empty = match field {
            NewConnectionField::Password => {
                form.empty_password
                    && (form.auth_tab == SshAuthTab::Password
                        || matches!(
                            form.transport,
                            NewConnectionTransport::Rdp | NewConnectionTransport::Vnc
                        ))
            }
            NewConnectionField::StandaloneSftpSecondaryPassword => {
                form.standalone_sftp_secondary.empty_password
            }
            NewConnectionField::JumpPassword => form
                .jump_server_form
                .as_ref()
                .is_some_and(|jump| jump.empty_password),
            _ => false,
        };
        if use_empty {
            let input = oxideterm_gpui_ui::text_input(
                &self.tokens,
                TextInputView {
                    value: "",
                    placeholder,
                    focused: false,
                    caret_visible: false,
                    secret: true,
                    selected_all: false,
                    selected_range: None,
                    marked_text: None,
                },
            );
            return Some((div().opacity(0.5).child(input).into_any_element(), None));
        }
        let value = connection_secret_field_value(form, field)?;
        let visible = connection_secret_field_visible(form, field);
        Some((
            self.render_connection_input(value, placeholder, field, !visible.unwrap_or(false), cx),
            visible,
        ))
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

    pub(super) fn render_edit_saved_password_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((input, _)) = self.render_connection_secret_input(
            self.i18n
                .t("sessionManager.edit_properties.password_placeholder"),
            NewConnectionField::Password,
            cx,
        ) else {
            return div().into_any_element();
        };
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return div().into_any_element();
        };
        if self.environment.browser.is_some() {
            return form_field(&self.tokens, self.i18n.t("ssh.form.password"), input);
        }
        let loading = form.password_load_id.is_some();
        let error = form.password_load_error;
        let button = if loading {
            oxideterm_gpui_ui::button::icon_button(
                &self.tokens,
                self.render_loading_icon(
                    "saved-password-loading",
                    SECRET_VISIBILITY_ICON_SIZE,
                    rgb(self.tokens.ui.text_muted),
                ),
                IconButtonOptions {
                    loading: true,
                    ..IconButtonOptions::opaque_toolbar(
                        SECRET_VISIBILITY_BUTTON_SIZE,
                        ButtonRadius::Sm,
                    )
                },
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        } else {
            self.workspace_icon_action_button(
                LucideIcon::Eye,
                SECRET_VISIBILITY_ICON_SIZE,
                rgb(self.tokens.ui.text_muted),
                IconButtonOptions {
                    hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                    ..IconButtonOptions::opaque_toolbar(
                        SECRET_VISIBILITY_BUTTON_SIZE,
                        ButtonRadius::Sm,
                    )
                },
                |this, _, _, cx| {
                    this.reveal_edit_saved_password(cx);
                    cx.stop_propagation();
                },
                cx,
            )
        };
        let control = div().relative().child(input).child(
            button
                .absolute()
                .right(px(SECRET_VISIBILITY_BUTTON_OFFSET))
                .top(px(SECRET_VISIBILITY_BUTTON_OFFSET)),
        );
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(form_field(
                &self.tokens,
                self.i18n.t("sessionManager.edit_properties.saved_password"),
                control,
            ))
            .when_some(error, |this, error| {
                this.child(self.render_connection_hint_with_color(
                    self.i18n.t(error.message_key()),
                    self.tokens.ui.error,
                ))
            })
            .into_any_element()
    }

    fn reveal_edit_saved_password(&mut self, cx: &mut Context<Self>) {
        self.request(FormAction::RevealSavedPassword, None, cx);
    }

    pub(super) fn render_connection_field_with_browse(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.environment.browser.is_some() {
            return form_field(
                &self.tokens,
                label,
                self.render_connection_input(value, placeholder, field, false, cx),
            );
        }
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
        super::super::select::group_select(
            &self.flow,
            &self.dispatch,
            &self.tokens,
            &self.i18n,
            label,
            value,
            cx,
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn render_managed_key_select(
        &self,
        label: String,
        selected_id: &str,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = if jump_form {
            NewConnectionSelect::JumpManagedKey
        } else {
            NewConnectionSelect::ManagedKey
        };
        self.render_managed_key_select_for_target(label, selected_id, select_id, cx)
    }

    pub(super) fn render_connection_totp_select(
        &self,
        jump: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_none_or(|form| form.transport != NewConnectionTransport::Ssh)
        {
            return div().into_any_element();
        }
        let select = if jump {
            NewConnectionSelect::JumpTotp
        } else {
            NewConnectionSelect::Totp
        };
        let id = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| {
                if jump {
                    form.jump_server_form
                        .as_ref()
                        .and_then(|hop| hop.totp_credential_id.as_deref())
                } else {
                    form.totp_credential_id.as_deref()
                }
            });
        let label = id
            .map(|id| {
                self.environment
                    .totp
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.name.clone())
                    .unwrap_or_else(|| self.i18n.t("settings_view.totp.unavailable"))
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.totp.none"));
        let trigger = self
            .new_connection_select_trigger(select, label, id.is_none(), false, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = &mut state.form {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.open_new_connection_select_from_pointer(select, cx);
                    this.request(FormAction::FocusForm, Some(window), cx);
                    cx.stop_propagation();
                }),
            );
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(form_field(
                &self.tokens,
                self.i18n.t("settings_view.totp.connection_label"),
                self.track_new_connection_select_anchor(select, trigger, cx),
            ))
            .child(self.render_connection_hint(self.i18n.t("settings_view.totp.connection_hint")))
            .into_any_element()
    }

    pub(super) fn render_standalone_sftp_secondary_managed_key_select(
        &self,
        label: String,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_managed_key_select_for_target(
            label,
            selected_id,
            NewConnectionSelect::StandaloneSftpSecondaryManagedKey,
            cx,
        )
    }

    fn render_managed_key_select_for_target(
        &self,
        label: String,
        selected_id: &str,
        select_id: NewConnectionSelect,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let keys = self.environment.keys.clone();
        let selected_label = if selected_id.trim().is_empty() {
            self.i18n.t("ssh.form.managed_key_placeholder")
        } else {
            keys.iter()
                .find(|key| key.id == selected_id)
                .map(|key| key.name.clone())
                .unwrap_or_else(|| selected_id.to_string())
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
                    if this.environment.keys.clone().is_empty() {
                        cx.stop_propagation();
                        return;
                    }
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.request(FormAction::ClearComposition, None, cx);
                    this.open_new_connection_select_from_pointer(select_id, cx);
                    this.request(FormAction::FocusForm, Some(window), cx);
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
        let connections = self.environment.connections.clone();
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
                    this.request(FormAction::ClearComposition, None, cx);
                    this.open_new_connection_select_from_pointer(
                        NewConnectionSelect::JumpSavedConnection,
                        cx,
                    );
                    this.request(FormAction::FocusForm, Some(window), cx);
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
        let connections = self.environment.connections.clone();
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
                    if this.environment.connections.is_empty() && !gateway_selected {
                        cx.stop_propagation();
                        return;
                    }
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                    });
                    this.request(FormAction::ClearComposition, None, cx);
                    this.open_new_connection_select_from_pointer(
                        NewConnectionSelect::RemoteDesktopSshGateway,
                        cx,
                    );
                    this.request(FormAction::FocusForm, Some(window), cx);
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
                    NewConnectionSelect::StandaloneSftpSecondaryManagedKey => {
                        form.standalone_sftp_secondary.managed_key_id = key_id;
                        form.focused_field =
                            NewConnectionField::StandaloneSftpSecondaryManagedKeyId;
                    }
                    NewConnectionSelect::JumpManagedKey => {
                        let Some(jump_form) = form.jump_server_form.as_mut() else {
                            return;
                        };
                        jump_form.managed_key_id = key_id;
                        form.focused_field = NewConnectionField::JumpManagedKeyId;
                    }
                    NewConnectionSelect::Group
                    | NewConnectionSelect::Totp
                    | NewConnectionSelect::JumpTotp
                    | NewConnectionSelect::KeyAuthSource
                    | NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource
                    | NewConnectionSelect::JumpSavedConnection
                    | NewConnectionSelect::TunnelHost
                    | NewConnectionSelect::RemoteDesktopSshGateway
                    | NewConnectionSelect::JumpKeyAuthSource
                    | NewConnectionSelect::UpstreamProxyPolicy
                    | NewConnectionSelect::UpstreamProxyProtocol
                    | NewConnectionSelect::UpstreamProxyAuth
                    | NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyPolicy
                    | NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyProtocol
                    | NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyAuth
                    | NewConnectionSelect::LocalShell
                    | NewConnectionSelect::SerialPort
                    | NewConnectionSelect::BleAdapter
                    | NewConnectionSelect::SerialDataBits
                    | NewConnectionSelect::SerialStopBits
                    | NewConnectionSelect::SerialParity
                    | NewConnectionSelect::SerialFlowControl
                    | NewConnectionSelect::TerminalEncoding
                    | NewConnectionSelect::TerminalBackspaceSequence
                    | NewConnectionSelect::TerminalDeleteSequence
                    | NewConnectionSelect::TelnetInputLineEnding
                    | NewConnectionSelect::TelnetOutputLineEnding
                    | NewConnectionSelect::BleInputLineEnding
                    | NewConnectionSelect::BleOutputLineEnding
                    | NewConnectionSelect::BleSendMode
                    | NewConnectionSelect::BleDisplayMode
                    | NewConnectionSelect::TerminalSemanticScheme
                    | NewConnectionSelect::TerminalHighlightRuleSet
                    | NewConnectionSelect::TerminalSessionLogPolicy
                    | NewConnectionSelect::TerminalBellPolicy
                    | NewConnectionSelect::X11ForwardingMode
                    | NewConnectionSelect::X11UntrustedTimeout
                    | NewConnectionSelect::PrimaryActionMenu => return,
                }
                form.field_focused = false;
                form.selected_field = None;
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_jump_saved_connection(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let selected_connection = self
            .environment
            .connections
            .clone()
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn connection_form_group_options(&self, current_group: &str) -> Vec<String> {
        let mut groups = self.environment.groups.clone();
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

    pub(in crate::views) fn connection_form_group_is_ungrouped(&self, group: &str) -> bool {
        crate::form_state::connection_group_is_ungrouped(group, &self.i18n)
    }

    pub(super) fn connection_form_ungrouped_label(&self) -> String {
        self.i18n.t("ssh.form.ungrouped")
    }

    pub(super) fn render_proxy_command_section(
        &self,
        enabled: bool,
        command: &str,
        configured: bool,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(self.render_connection_switch_row(
                format!("proxy-command-{secondary}"),
                self.i18n.t("ssh.form.proxy_command_enable"),
                enabled,
                false,
                move |form| {
                    if secondary {
                        let route = &mut form.standalone_sftp_secondary;
                        route.proxy_command_enabled = !route.proxy_command_enabled;
                    } else {
                        form.proxy_command_enabled = !form.proxy_command_enabled;
                    }
                },
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
                        if secondary {
                            NewConnectionField::StandaloneSftpSecondaryProxyCommand
                        } else {
                            NewConnectionField::ProxyCommand
                        },
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
                | NewConnectionField::StandaloneSftpSecondaryKeyPath
                | NewConnectionField::StandaloneSftpSecondaryCertPath
                | NewConnectionField::JumpKeyPath
                | NewConnectionField::JumpCertPath
        ) {
            return;
        }
        (self.pick_path)(field, &self.i18n, cx);
    }

    pub(super) fn render_connection_input(
        &self,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &App,
    ) -> AnyElement {
        let target = ConnectionsInput::Form(field);
        let Some(frame) = self.frame(target, cx) else {
            return div().into_any_element();
        };
        let form = self.connection_form_state(cx).form.as_ref();
        form_text_input(
            &self.tokens,
            &self.dispatch,
            target,
            frame,
            TextInputView {
                value,
                placeholder,
                focused: form.is_some_and(|form| form.field_focused && form.focused_field == field),
                caret_visible: false,
                secret,
                selected_all: form.is_some_and(|form| connection_field_is_selected(form, field)),
                selected_range: None,
                marked_text: None,
            },
        )
    }

    pub(super) fn render_connection_centered_mono_input(
        &self,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = ConnectionsInput::Form(field);
        let Some(frame) = self.frame(target, cx) else {
            return div().into_any_element();
        };
        let form = self.connection_form_state(cx).form.as_ref();
        let focused = form.is_some_and(|form| form.field_focused && form.focused_field == field);
        let mut input = text_input_with_viewport_and_content_align(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: frame.caret_visible,
                secret: false,
                selected_all: form.is_some_and(|form| connection_field_is_selected(form, field)),
                selected_range: frame.selected_range,
                marked_text: frame.marked_text.as_ref().map(|text| text.as_str()),
            },
            TextInputContentAlign::Center,
            &frame.viewport,
            frame.active_offset,
        )
        .font_family(SharedString::from(
            self.tokens.metrics.markdown_code_font_family,
        ));
        if !focused {
            input = input.border_color(rgba((self.tokens.ui.border << 8) | 0xb8));
        }
        self.finish_connection_input(input, field, cx)
    }

    fn finish_connection_input(
        &self,
        input: Div,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = ConnectionsInput::Form(field);
        let Some(frame) = self.frame(target, cx) else {
            return input.into_any_element();
        };
        crate::views::bind_connection_input(
            input,
            target,
            frame.anchor_id,
            frame.update_anchor,
            &self.dispatch,
        )
    }

    fn render_connection_mono_input(
        &self,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        align: TextInputContentAlign,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = ConnectionsInput::Form(field);
        let Some(frame) = self.frame(target, cx) else {
            return div().into_any_element();
        };
        let form = self.connection_form_state(cx).form.as_ref();
        let focused = form.is_some_and(|form| form.field_focused && form.focused_field == field);
        let input = text_input_with_viewport_and_content_align(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: frame.caret_visible,
                secret: false,
                selected_all: form.is_some_and(|form| connection_field_is_selected(form, field)),
                selected_range: frame.selected_range,
                marked_text: frame.marked_text.as_ref().map(|text| text.as_str()),
            },
            align,
            &frame.viewport,
            frame.active_offset,
        )
        .font_family(SharedString::from(
            self.tokens.metrics.markdown_code_font_family,
        ));
        self.finish_connection_input(input, field, cx)
    }

    fn render_endpoint_separator_symbol(&self, glyph: &'static str, width: f32) -> Div {
        div()
            .w(px(width))
            .h(px(self.tokens.metrics.ui_control_height))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .font_family(SharedString::from(
                self.tokens.metrics.markdown_code_font_family,
            ))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(glyph)
    }

    /// Clickable connection avatar badge that opens the Appearance Popover directly
    /// from the Basic identity row.
    pub(super) fn render_identity_avatar_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let avatar_size = self.tokens.metrics.ui_control_height;
        div()
            .id("ssh-identity-avatar-trigger")
            .flex_none()
            .cursor_pointer()
            .rounded(px(self.tokens.radii.md))
            .hover(|el| el.opacity(0.88))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.close_new_connection_select(cx);
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.appearance_popover_open = !form.appearance_popover_open;
                            form.icon_picker_expanded = true;
                        }
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(self.render_identity_avatar_preview(avatar_size, 16.0, cx))
            .into_any_element()
    }

    /// Responsive identity row (`[ Avatar ] [ Name ] [ Group ]`) that stacks Group
    /// onto a full-width second line on narrow viewports.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_identity_row(
        &self,
        name_label: String,
        name_value: &str,
        name_placeholder: String,
        name_field: NewConnectionField,
        group: Option<(String, &str)>,
        show_avatar: bool,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let avatar = show_avatar.then(|| self.render_identity_avatar_button(cx));
        let name_control = self.render_connection_field(
            name_label,
            name_value,
            name_placeholder,
            name_field,
            false,
            cx,
        );
        let Some((group_label, group_value)) = group else {
            return div()
                .w_full()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(10.0))
                .when_some(avatar, |row, trigger| row.child(trigger))
                .child(div().flex_1().min_w_0().child(name_control))
                .into_any_element();
        };
        let group_control = self.render_connection_group_select(group_label, group_value, cx);
        if layout.narrow_content {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(10.0))
                        .when_some(avatar, |row, trigger| row.child(trigger))
                        .child(div().flex_1().min_w_0().child(name_control)),
                )
                .child(div().w_full().child(group_control))
                .into_any_element()
        } else {
            let group_width = (layout.content_width * 0.30).clamp(148.0, 220.0).round();
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(12.0))
                .when_some(avatar, |row, trigger| row.child(trigger))
                .child(div().flex_1().min_w_0().child(name_control))
                .child(div().w(px(group_width)).flex_none().child(group_control))
                .into_any_element()
        }
    }

    /// Shared sunken segmented control track whose inner height stays strictly flush
    /// with child tabs to prevent capsule overflow.
    pub(super) fn render_sunken_segmented_track(&self) -> Div {
        let theme = self.tokens.ui;
        segmented_tabs(&self.tokens)
            .h(px(self.tokens.metrics.ui_tabs_list_height + 2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_sunken << 8) | 0xc8))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .p(px(2.0))
    }

    pub(super) fn render_sunken_segmented_tab(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        selected: bool,
    ) -> gpui::Stateful<Div> {
        let theme = self.tokens.ui;
        segmented_tab(&self.tokens, label, selected)
            .id(id)
            .h_full()
            .min_h_0()
            .py_0()
            .rounded(px(self.tokens.radii.sm))
            .bg(if selected {
                rgb(theme.bg_card)
            } else {
                rgba(0x00000000)
            })
            .when(selected, |el| {
                el.border_1().border_color(rgba((theme.border << 8) | 0xc0))
            })
    }

    /// Separate monospace `host` and `port` inputs with a standalone `:`
    /// separator between them (Telnet, RDP, VNC, SSH tunnel endpoints).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_unified_host_port_bar(
        &self,
        host_label: String,
        host_field: NewConnectionField,
        host: &str,
        host_placeholder: String,
        port_label: String,
        port_field: NewConnectionField,
        port: &str,
        port_placeholder: String,
        content_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let port_width = (content_width * 0.16).clamp(68.0, 96.0).round();
        let host_control = form_field(
            &self.tokens,
            host_label,
            self.render_connection_mono_input(
                host,
                host_placeholder,
                host_field,
                TextInputContentAlign::Start,
                cx,
            ),
        );
        let port_control = form_field(
            &self.tokens,
            port_label,
            self.render_connection_mono_input(
                port,
                port_placeholder,
                port_field,
                TextInputContentAlign::Center,
                cx,
            ),
        );

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_end()
            .child(div().flex_1().min_w_0().child(host_control))
            .child(self.render_endpoint_separator_symbol(":", 18.0))
            .child(div().w(px(port_width)).flex_none().child(port_control))
            .into_any_element()
    }

    /// Separate monospace `username`, `host`, and `port` inputs with standalone
    /// `@` and `:` symbols between them. Folds cleanly onto two rows on narrow
    /// panes to preserve host readability.
    pub(super) fn render_unified_address_bar(
        &self,
        username: &str,
        host: &str,
        host_placeholder: String,
        port: &str,
        content_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_unified_address_bar_with_fields(
            NewConnectionField::Username,
            username,
            "root".to_string(),
            NewConnectionField::Host,
            host,
            host_placeholder,
            NewConnectionField::Port,
            port,
            SSH_DEFAULT_PORT_TEXT.to_string(),
            content_width,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_unified_address_bar_with_fields(
        &self,
        username_field: NewConnectionField,
        username: &str,
        username_placeholder: String,
        host_field: NewConnectionField,
        host: &str,
        host_placeholder: String,
        port_field: NewConnectionField,
        port: &str,
        port_placeholder: String,
        content_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let username_control = form_field(
            &self.tokens,
            self.i18n.t("ssh.form.username"),
            self.render_connection_mono_input(
                username,
                username_placeholder,
                username_field,
                TextInputContentAlign::Start,
                cx,
            ),
        );
        let host_control = form_field(
            &self.tokens,
            self.i18n.t("ssh.form.host"),
            self.render_connection_mono_input(
                host,
                host_placeholder,
                host_field,
                TextInputContentAlign::Start,
                cx,
            ),
        );
        let port_control = form_field(
            &self.tokens,
            self.i18n.t("ssh.form.port"),
            self.render_connection_mono_input(
                port,
                port_placeholder,
                port_field,
                TextInputContentAlign::Center,
                cx,
            ),
        );

        if content_width < 420.0 {
            let port_width = 72.0;
            return div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(div().w_full().min_w_0().child(host_control))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(10.0))
                        .child(div().flex_1().min_w_0().child(username_control))
                        .child(div().w(px(port_width)).flex_none().child(port_control)),
                )
                .into_any_element();
        }

        let user_width = (content_width * 0.24).clamp(96.0, 168.0).round();
        let port_width = (content_width * 0.12).clamp(64.0, 88.0).round();

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_end()
            .child(div().w(px(user_width)).flex_none().child(username_control))
            .child(self.render_endpoint_separator_symbol("@", 22.0))
            .child(div().flex_1().min_w_0().child(host_control))
            .child(self.render_endpoint_separator_symbol(":", 18.0))
            .child(div().w(px(port_width)).flex_none().child(port_control))
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
            AuthSelectorContext::Standard
            | AuthSelectorContext::StandaloneSftpSecondary
            | AuthSelectorContext::Prompt => self.i18n.t("ssh.form.authentication"),
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
            .h_full()
            .min_h_0()
            .py_0()
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
        let theme = self.tokens.ui;
        let item_width = 1.0 / choices.len().max(1) as f32;
        let active_left = active_index as f32 * item_width;
        let previous_left = previous_index as f32 * item_width;
        let indicator = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(gpui::relative(item_width))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(theme.bg_card))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0xc0));
        let indicator = oxideterm_gpui_ui::segmented_control::selection_indicator(
            &self.tokens,
            control_id,
            indicator,
            previous_left,
            active_left,
            transition_generation.is_some(),
        );
        let mut inner = div()
            .relative()
            .w_full()
            .h_full()
            .flex()
            .flex_row()
            .child(indicator);
        for item in items {
            inner = inner.child(item);
        }
        // Use a sunken pill track with an elevated selected capsule so the active
        // authentication method stays clearly separated from inputs below it.
        let row = segmented_tabs(&self.tokens)
            .h(px(self.tokens.metrics.ui_tabs_list_height + 2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_sunken << 8) | 0xc8))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .p(px(2.0))
            .child(inner);

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

    pub(super) fn render_standalone_sftp_endpoint_auth(
        &self,
        active_tab: SshAuthTab,
        secondary: bool,
        key_path: &str,
        managed_key_id: &str,
        cert_path: &str,
        identity_agent: &str,
        gssapi_enabled: bool,
        gssapi_server_identity: &str,
        gssapi_delegate_credentials: bool,
        gssapi_credentials_available: Option<bool>,
        agent_available: Option<bool>,
        saved_credential_present: bool,
        save_password: bool,
        empty_password: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = if secondary {
            AuthSelectorContext::StandaloneSftpSecondary
        } else {
            AuthSelectorContext::Standard
        };
        let password_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryPassword
        } else {
            NewConnectionField::Password
        };
        let key_path_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryKeyPath
        } else {
            NewConnectionField::KeyPath
        };
        let cert_path_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryCertPath
        } else {
            NewConnectionField::CertPath
        };
        let passphrase_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryPassphrase
        } else {
            NewConnectionField::Passphrase
        };
        let identity_agent_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryIdentityAgent
        } else {
            NewConnectionField::IdentityAgent
        };
        let gssapi_server_identity_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity
        } else {
            NewConnectionField::GssapiServerIdentity
        };

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_auth_selector(active_tab, context, false, cx))
            .when(active_tab == SshAuthTab::Password, |content| {
                content
                    .child(self.render_connection_secret_field(
                        self.i18n.t("ssh.form.password"),
                        String::new(),
                        password_field,
                        cx,
                    ))
                    .when(saved_credential_present, |content| {
                        content.child(self.render_connection_hint(
                            self.i18n.t("sessionManager.edit_properties.password_hint"),
                        ))
                    })
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .when(!saved_credential_present, |rows| {
                                rows.child(self.render_connection_switch_row(
                                    format!("sftp-save-password-{secondary}"),
                                    self.i18n.t("ssh.form.save_password"),
                                    save_password,
                                    false,
                                    if secondary {
                                        toggle_secondary_sftp_password_persistence
                                    } else {
                                        toggle_primary_sftp_password_persistence
                                    },
                                    cx,
                                ))
                                .child(self.form_hairline())
                            })
                            .child(self.render_connection_switch_row(
                                format!("sftp-empty-password-{secondary}"),
                                self.i18n.t("ssh.form.use_empty_password"),
                                empty_password,
                                false,
                                if secondary {
                                    toggle_secondary_sftp_empty_password
                                } else {
                                    toggle_primary_sftp_empty_password
                                },
                                cx,
                            )),
                    )
            })
            .when(active_tab == SshAuthTab::DefaultKey, |content| {
                content
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.default_key_desc")))
                    .child(self.render_connection_secret_field(
                        self.i18n.t("ssh.form.passphrase"),
                        self.i18n.t("ssh.form.passphrase_placeholder"),
                        passphrase_field,
                        cx,
                    ))
            })
            .when(active_tab == SshAuthTab::SshKey, |content| {
                content
                    .child(self.render_connection_field_with_browse(
                        self.i18n.t("ssh.form.key_file"),
                        key_path,
                        "~/.ssh/id_ed25519".to_string(),
                        key_path_field,
                        cx,
                    ))
                    .child(self.render_connection_secret_field(
                        self.i18n.t("ssh.form.passphrase"),
                        self.i18n.t("ssh.form.passphrase_placeholder"),
                        passphrase_field,
                        cx,
                    ))
            })
            .when(active_tab == SshAuthTab::ManagedKey, |content| {
                let managed_key = if secondary {
                    self.render_standalone_sftp_secondary_managed_key_select(
                        self.i18n.t("ssh.form.managed_key"),
                        managed_key_id,
                        cx,
                    )
                } else {
                    self.render_managed_key_select(
                        self.i18n.t("ssh.form.managed_key"),
                        managed_key_id,
                        false,
                        cx,
                    )
                };
                content
                    .child(managed_key)
                    .child(self.render_connection_secret_field(
                        self.i18n.t("ssh.form.passphrase"),
                        self.i18n.t("ssh.form.passphrase_placeholder"),
                        passphrase_field,
                        cx,
                    ))
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.managed_key_hint")))
            })
            .when(active_tab == SshAuthTab::Certificate, |content| {
                content
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.certificate_note")))
                    .child(self.render_connection_field_with_browse(
                        self.i18n.t("ssh.form.private_key"),
                        key_path,
                        "~/.ssh/id_ed25519".to_string(),
                        key_path_field,
                        cx,
                    ))
                    .child(self.render_connection_field_with_browse(
                        self.i18n.t("ssh.form.certificate"),
                        cert_path,
                        "~/.ssh/id_ed25519-cert.pub".to_string(),
                        cert_path_field,
                        cx,
                    ))
                    .child(self.render_connection_secret_field(
                        self.i18n.t("ssh.form.passphrase"),
                        self.i18n.t("ssh.form.passphrase_placeholder"),
                        passphrase_field,
                        cx,
                    ))
            })
            .when(active_tab == SshAuthTab::Agent, |content| {
                content
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.agent_desc")))
                    .child(self.render_connection_field(
                        self.i18n.t("ssh.form.agent_endpoint"),
                        identity_agent,
                        self.i18n.t("ssh.form.agent_endpoint_placeholder"),
                        identity_agent_field,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.agent_endpoint_hint")))
                    .child(self.render_agent_status(agent_available))
            })
            .when(active_tab == SshAuthTab::TwoFactor, |content| {
                content
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.two_factor_desc")))
                    .child(self.render_connection_hint(self.i18n.t("ssh.form.two_factor_hint")))
                    .child(self.render_connection_hint_with_color(
                        self.i18n.t("ssh.form.two_factor_warning"),
                        self.tokens.ui.warning,
                    ))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(self.form_hairline())
                    .child(self.render_connection_switch_row(
                        format!("sftp-kerberos-{secondary}"),
                        self.i18n.t("ssh.form.kerberos_preferred"),
                        gssapi_enabled,
                        false,
                        if secondary {
                            toggle_secondary_sftp_gssapi
                        } else {
                            toggle_primary_sftp_gssapi
                        },
                        cx,
                    ))
                    .when(gssapi_enabled, |content| {
                        content
                            .child(self.render_connection_hint(self.i18n.t("ssh.form.gssapi_desc")))
                            .child(self.render_connection_field(
                                self.i18n.t("ssh.form.gssapi_server_identity"),
                                gssapi_server_identity,
                                self.i18n.t("ssh.form.gssapi_server_identity_placeholder"),
                                gssapi_server_identity_field,
                                false,
                                cx,
                            ))
                            .child(
                                self.render_kerberos_credentials_status(
                                    gssapi_credentials_available,
                                ),
                            )
                            .child(self.render_connection_switch_row_with_warning(
                                if secondary {
                                    "secondary-sftp-kerberos-delegation-help"
                                } else {
                                    "primary-sftp-kerberos-delegation-help"
                                },
                                if secondary {
                                    "secondary-sftp-kerberos-delegation-tooltip"
                                } else {
                                    "primary-sftp-kerberos-delegation-tooltip"
                                },
                                "ssh.form.gssapi_delegate_credentials",
                                "ssh.form.gssapi_delegation_warning",
                                gssapi_delegate_credentials,
                                false,
                                if secondary {
                                    toggle_secondary_sftp_gssapi_delegation
                                } else {
                                    toggle_primary_sftp_gssapi_delegation
                                },
                                cx,
                            ))
                    }),
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
            AuthSelectorContext::Standard | AuthSelectorContext::StandaloneSftpSecondary => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
                (SshAuthFamily::TwoFactor, "ssh.auth.two_factor"),
            ],
        }
    }

    fn auth_selector_motion_id(context: AuthSelectorContext) -> &'static str {
        match context {
            AuthSelectorContext::Standard => NEW_CONNECTION_AUTH_SELECTOR_ID,
            AuthSelectorContext::StandaloneSftpSecondary => {
                "standalone-sftp-secondary-auth-selector"
            }
            AuthSelectorContext::EditProperties => EDIT_CONNECTION_AUTH_SELECTOR_ID,
            AuthSelectorContext::Prompt => PROMPT_CONNECTION_AUTH_SELECTOR_ID,
            AuthSelectorContext::DrillDown => DRILL_DOWN_AUTH_SELECTOR_ID,
            AuthSelectorContext::Jump => JUMP_CONNECTION_AUTH_SELECTOR_ID,
        }
    }

    pub(super) fn key_auth_source_choices(
        context: AuthSelectorContext,
    ) -> &'static [SshKeyAuthSource] {
        match context {
            AuthSelectorContext::Standard
            | AuthSelectorContext::StandaloneSftpSecondary
            | AuthSelectorContext::Jump => &[
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
        let select_id = if context == AuthSelectorContext::StandaloneSftpSecondary {
            NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource
        } else if jump_form {
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
                    this.request(FormAction::ClearComposition, None, cx);
                    this.open_new_connection_select_from_pointer(select_id, cx);
                    this.request(FormAction::FocusForm, Some(window), cx);
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
                let current_tab = if context == AuthSelectorContext::StandaloneSftpSecondary {
                    form.standalone_sftp_secondary.auth_tab
                } else if jump_form {
                    form.jump_server_form
                        .as_ref()
                        .map(|jump_form| jump_form.auth_tab)
                        .unwrap_or(SshAuthTab::Password)
                } else {
                    form.auth_tab
                };
                let next_tab = Self::auth_tab_for_family_selection(family, current_tab, context);
                if context == AuthSelectorContext::StandaloneSftpSecondary {
                    form.standalone_sftp_secondary.auth_tab = next_tab;
                } else if jump_form {
                    if let Some(jump_form) = form.jump_server_form.as_mut() {
                        jump_form.auth_tab = next_tab;
                    }
                } else {
                    form.auth_tab = next_tab;
                }
                form.focused_field = Self::focus_field_for_auth_tab(
                    next_tab,
                    jump_form,
                    context == AuthSelectorContext::StandaloneSftpSecondary,
                );
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.close_new_connection_select(cx);
        self.request(FormAction::ClearComposition, None, cx);
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
                    NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource => {
                        form.standalone_sftp_secondary.auth_tab = tab;
                    }
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
                    select_id == NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource,
                );
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    fn focus_field_for_auth_tab(
        tab: SshAuthTab,
        jump_form: bool,
        standalone_sftp_secondary: bool,
    ) -> NewConnectionField {
        if standalone_sftp_secondary {
            match tab {
                SshAuthTab::Password => NewConnectionField::StandaloneSftpSecondaryPassword,
                SshAuthTab::SshKey | SshAuthTab::Certificate => {
                    NewConnectionField::StandaloneSftpSecondaryKeyPath
                }
                SshAuthTab::ManagedKey => NewConnectionField::StandaloneSftpSecondaryManagedKeyId,
                SshAuthTab::DefaultKey => NewConnectionField::StandaloneSftpSecondaryPassphrase,
                SshAuthTab::Agent | SshAuthTab::TwoFactor => {
                    NewConnectionField::StandaloneSftpSecondaryHost
                }
            }
        } else if jump_form {
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
        supports_auto_icon: bool,
        default_icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let preview_color = parse_rgb24_hex(color_value).unwrap_or(theme.accent);
        let preview_background = parse_rgb24_hex(background_color_value)
            .map(rgb)
            .unwrap_or_else(|| rgba((preview_color << 8) | 0x22));
        // An empty (or unknown) icon id means "use the transport default"; the
        // preview must show that same icon so clearing the picker is visible.
        let active_icon = session_icon_from_id(Some(icon_value)).unwrap_or(default_icon.into());
        let mut grid = div().flex().flex_wrap().gap(px(self.tokens.spacing.two));

        let choices = DISTRO_ICONS
            .iter()
            .map(|icon| (icon.id, icon.name, SessionIcon::Distro(icon)))
            .chain(
                SESSION_ICON_CHOICES
                    .iter()
                    .map(|choice| (choice.id, choice.id, choice.icon.into())),
            );
        for (id, name, icon) in choices {
            let selected = icon_value.trim() == id;
            let icon_id = id.to_string();
            let tokens = self.tokens;
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
                    .id(gpui::SharedString::from(format!(
                        "session-icon-choice-{id}"
                    )))
                    .when(matches!(icon, SessionIcon::Distro(_)), |button| {
                        button.aria_label(name).tooltip(move |_, cx| {
                            oxideterm_gpui_ui::tooltip::tooltip_view(
                                tokens,
                                name.to_owned(),
                                None,
                                cx,
                            )
                        })
                    })
                    .child(icon.render(
                        18.0,
                        if selected {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                        selected,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.icon = icon_id.clone();
                                    if supports_auto_icon {
                                        form.icon_picker_expanded = false;
                                    }
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
                                .child(active_icon.render(18.0, rgb(preview_color), true)),
                        )
                        .child(
                            button(
                                &self.tokens,
                                if supports_auto_icon {
                                    self.i18n.t("sessionManager.edit_properties.custom_icon")
                                } else if expanded {
                                    self.i18n.t("sessionManager.edit_properties.hide_icons")
                                } else {
                                    self.i18n.t("sessionManager.edit_properties.choose_icon")
                                },
                                if supports_auto_icon && !icon_value.trim().is_empty() {
                                    ButtonTone::Primary
                                } else {
                                    ButtonTone::Secondary
                                },
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
                        .when(supports_auto_icon || !icon_value.trim().is_empty(), |row| {
                            row.child(
                                button(
                                    &self.tokens,
                                    self.i18n.t(if supports_auto_icon {
                                        "sessionManager.edit_properties.auto_icon"
                                    } else {
                                        "sessionManager.edit_properties.default_icon"
                                    }),
                                    if supports_auto_icon && icon_value.trim().is_empty() {
                                        ButtonTone::Primary
                                    } else {
                                        ButtonTone::Secondary
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.update_connection_form_state(cx, |state| {
                                            if let Some(form) = state.form.as_mut() {
                                                form.icon.clear();
                                                if supports_auto_icon {
                                                    form.icon_picker_expanded = false;
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
                .when(
                    supports_auto_icon && icon_value.trim().is_empty(),
                    |content| {
                        content.child(self.render_connection_hint(
                            self.i18n.t("sessionManager.edit_properties.auto_icon_hint"),
                        ))
                    },
                )
                .when(matches!(active_icon, SessionIcon::Distro(_)), |content| {
                    content.child(
                        self.render_connection_hint(
                            self.i18n
                                .t("sessionManager.edit_properties.brand_icon_hint"),
                        ),
                    )
                })
                .when(expanded, |content| {
                    content.child(
                        div()
                            .id("edit-connection-icon-grid")
                            .max_h(px(180.0))
                            .overflow_y_scroll()
                            .track_scroll(
                                &self.selectable_text_scroll_handle("edit-connection-icon-grid"),
                            )
                            // The icon grid is a nested scroll surface inside
                            // the edit dialog. Wheel input over it should not
                            // also move the outer form body.
                            .on_scroll(|_, _, cx| cx.stop_propagation())
                            .child(grid),
                    )
                }),
        )
    }

    pub(super) fn render_local_terminal_form_branch(
        &self,
        active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::form_state::ConnectionFormDrawer;
        let form = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .expect("local form");
        let name = form.name.clone();
        let cwd = form.local_cwd.clone();
        let group = form.group.clone();
        let notes = form.notes.clone();
        let appearance_customized = !form.icon.trim().is_empty()
            || !form.color.trim().is_empty()
            || !form.icon_background_color.trim().is_empty();
        let selected_shell_id = form.local_shell_id.as_deref();
        if active_drawer == Some(ConnectionFormDrawer::Notes) {
            return div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_appearance_options(cx))
                .child(self.form_hairline())
                .child(self.render_connection_notes_fields(&notes, cx))
                .into_any_element();
        }
        let resolved_shell = match selected_shell_id {
            Some(id) => self
                .environment
                .local_shells
                .iter()
                .cloned()
                .into_iter()
                .find(|shell| shell.id == id),
            None => self
                .environment
                .local_shells
                .iter()
                .find(|shell| {
                    Some(shell.id.as_str()) == self.environment.default_shell_id.as_deref()
                })
                .or_else(|| self.environment.local_shells.first())
                .cloned(),
        };
        let default_shell_id = self.environment.default_shell_id.as_deref();
        let selected_label = resolved_shell
            .as_ref()
            .map(|shell| {
                if default_shell_id == Some(shell.id.as_str()) {
                    format!(
                        "{} · {}",
                        shell.label,
                        self.i18n.t("settings_view.local_terminal.default")
                    )
                } else {
                    shell.label.clone()
                }
            })
            .unwrap_or_else(|| {
                selected_shell_id
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.i18n.t("local_session.inherit_shell"))
            });
        let selected_label = if selected_shell_id.is_none() {
            self.i18n.t("local_session.inherit_shell")
        } else {
            selected_label
        };
        let selected_path = resolved_shell.as_ref().map(|shell| {
            format!(
                "{}: {}",
                self.i18n.t("settings_view.local_terminal.path"),
                shell.path.display()
            )
        });
        let shell_field = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(form_field(
                &self.tokens,
                self.i18n.t("settings_view.local_terminal.select_shell"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::LocalShell,
                    selected_label,
                    resolved_shell.is_none(),
                    false,
                    cx,
                ),
            ))
            .when_some(selected_path, |field, path| {
                field.child(self.render_connection_hint(path))
            })
            .into_any_element();
        let notes_trimmed = notes.trim();
        let notes_highlighted = !notes_trimmed.is_empty() || appearance_customized;
        let notes_summary = if !notes_trimmed.is_empty() {
            notes_trimmed
                .lines()
                .next()
                .unwrap_or(notes_trimmed)
                .to_string()
        } else if appearance_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_identity_row(
                self.i18n.t("ssh.form.name"),
                &name,
                self.i18n.t("ssh.form.name_placeholder"),
                NewConnectionField::Name,
                Some((self.i18n.t("ssh.form.group"), &group)),
                true,
                layout,
                cx,
            ))
            .child(self.form_hairline())
            .child(shell_field)
            .child(self.render_connection_field(
                self.i18n.t("local_session.directory"),
                &cwd,
                self.i18n.t("local_session.inherit_directory"),
                NewConnectionField::LocalCwd,
                false,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_connection_pre_connect_command(cx))
            .child(self.form_hairline())
            .child(self.render_connection_disclosure_row(
                "local-summary-Notes",
                LucideIcon::Sparkles,
                self.i18n.t("ssh.form.appearance"),
                notes_summary,
                notes_highlighted,
                ConnectionFormDrawer::Notes,
                cx,
            ))
            .into_any_element()
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

    pub(super) fn render_mosh_plugin_status(&self, cx: &mut Context<Self>) -> AnyElement {
        let available = self.environment.mosh_available;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(self.render_connection_hint_with_color(
                if available {
                    self.i18n.t("mosh_plugin.ready")
                } else {
                    self.plugin_requirement_message("com.oxideterm.terminal.mosh", "Mosh", cx)
                },
                if available {
                    self.tokens.ui.success
                } else {
                    self.tokens.ui.warning
                },
            ))
            .when(!available, |row| {
                row.child(self.workspace_toolbar_action_button(
                    self.i18n.t("remote_desktop.manage_plugin"),
                    None,
                    ToolbarButtonOptions::default(),
                    cx.listener(|this, _event, window, cx| {
                        this.close_new_connection_form(window, cx);
                        this.request(FormAction::OpenMoshPlugin, Some(window), cx);
                    }),
                ))
            })
            .into_any_element()
    }

    pub(super) fn render_mosh_advanced_fields(
        &self,
        server_executable: &str,
        udp_host: &str,
        udp_port: &str,
        locale: &str,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (
            ip_family,
            prediction,
            gssapi_enabled,
            gssapi_server_identity,
            gssapi_delegate_credentials,
            gssapi_credentials_available,
        ) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| {
                (
                    form.mosh_ip_family,
                    form.mosh_prediction,
                    form.gssapi_enabled,
                    form.gssapi_server_identity.clone(),
                    form.gssapi_delegate_credentials,
                    form.gssapi_credentials_available,
                )
            })
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
        let udp_host_field = self.render_connection_field(
            self.i18n.t("mosh.form.udp_host"),
            udp_host,
            self.i18n.t("mosh.form.udp_host_placeholder"),
            NewConnectionField::MoshUdpHost,
            false,
            cx,
        );
        let udp_port_field = self.render_connection_field(
            self.i18n.t("mosh.form.udp_port"),
            udp_port,
            self.i18n.t("mosh.form.udp_port_placeholder"),
            NewConnectionField::MoshUdpPort,
            false,
            cx,
        );
        let udp_row = if layout.narrow_content {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(udp_host_field)
                .child(udp_port_field)
        } else {
            div()
                .w_full()
                .flex()
                .flex_row()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(udp_host_field))
                .child(div().flex_1().min_w_0().child(udp_port_field))
        };

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_connection_field(
                self.i18n.t("mosh.form.server_executable"),
                server_executable,
                "mosh-server".to_string(),
                NewConnectionField::MoshServerExecutable,
                false,
                cx,
            ))
            .child(udp_row)
            .child(self.render_connection_hint(self.i18n.t("mosh.form.udp_port_hint")))
            .child(self.render_connection_field(
                self.i18n.t("mosh.form.locale"),
                locale,
                self.i18n.t("mosh.form.locale_placeholder"),
                NewConnectionField::MoshLocale,
                false,
                cx,
            ))
            .child(self.form_hairline())
            .child(form_field(
                &self.tokens,
                self.i18n.t("mosh.form.ip_family"),
                self.render_sunken_segmented_track().children(
                    ip_family_options
                        .into_iter()
                        .enumerate()
                        .map(|(index, (value, label_key))| {
                            self.render_sunken_segmented_tab(
                                SharedString::from(format!("mosh-ip-family-{index}")),
                                self.i18n.t(label_key),
                                ip_family == value,
                            )
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
                        }),
                ),
            ))
            .child(form_field(
                &self.tokens,
                self.i18n.t("mosh.form.prediction"),
                self.render_sunken_segmented_track().children(
                    prediction_options.into_iter().enumerate().map(
                        |(index, (value, label_key))| {
                            self.render_sunken_segmented_tab(
                                SharedString::from(format!("mosh-prediction-{index}")),
                                self.i18n.t(label_key),
                                prediction == value,
                            )
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
                ),
            ))
            .child(self.form_hairline())
            .child(self.render_connection_switch_row(
                "mosh-kerberos",
                self.i18n.t("ssh.form.kerberos_preferred"),
                gssapi_enabled,
                false,
                |form| {
                    form.gssapi_enabled = !form.gssapi_enabled;
                },
                cx,
            ))
            .when(gssapi_enabled, |sec| {
                sec.child(
                    div()
                        .pl(px(14.0))
                        .pb(px(4.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(self.render_connection_hint(self.i18n.t("ssh.form.gssapi_desc")))
                        .child(self.render_connection_field(
                            self.i18n.t("ssh.form.gssapi_server_identity"),
                            &gssapi_server_identity,
                            self.i18n.t("ssh.form.gssapi_server_identity_placeholder"),
                            NewConnectionField::GssapiServerIdentity,
                            false,
                            cx,
                        ))
                        .child(
                            self.render_kerberos_credentials_status(gssapi_credentials_available),
                        )
                        .child(self.render_connection_switch_row_with_warning(
                            "mosh-kerberos-delegation-help",
                            "mosh-kerberos-delegation-tooltip",
                            "ssh.form.gssapi_delegate_credentials",
                            "ssh.form.gssapi_delegation_warning",
                            gssapi_delegate_credentials,
                            false,
                            |form| {
                                form.gssapi_delegate_credentials =
                                    !form.gssapi_delegate_credentials;
                            },
                            cx,
                        )),
                )
            })
            .child(self.form_hairline())
            .child(self.render_connection_pre_connect_command(cx))
            .into_any_element()
    }

    pub(super) fn render_remote_desktop_form_branch(
        &self,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol,
        active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((
            name,
            host,
            port,
            username,
            keeps_saved_password,
            save_password,
            empty_password,
            group,
            notes,
            ssh_gateway_connection_id,
            upstream_proxy_policy,
            pre_connect_command,
            appearance_customized,
            vnc_options,
            session_options,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.name.clone(),
                form.host.clone(),
                form.port.clone(),
                form.username.clone(),
                form.remote_desktop_profile_id.is_some()
                    && form.saved_password_keychain_id.is_some(),
                form.save_password,
                form.empty_password,
                form.group.clone(),
                form.notes.clone(),
                form.remote_desktop_ssh_gateway_connection_id.clone(),
                form.upstream_proxy_policy,
                form.pre_connect_command.clone(),
                !form.icon.trim().is_empty()
                    || !form.color.trim().is_empty()
                    || !form.icon_background_color.trim().is_empty(),
                form.remote_desktop_session_options.vnc,
                form.remote_desktop_session_options.clone(),
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
            .gap(px(10.0))
            .child(self.render_identity_row(
                self.i18n.t("ssh.form.name"),
                &name,
                self.i18n.t("ssh.form.name_placeholder"),
                NewConnectionField::Name,
                Some((self.i18n.t("ssh.form.group"), &group)),
                true,
                layout,
                cx,
            ))
            .child(self.render_unified_host_port_bar(
                self.i18n.t("ssh.form.host"),
                NewConnectionField::Host,
                &host,
                self.i18n.t("ssh.form.host_placeholder"),
                self.i18n.t("ssh.form.port"),
                NewConnectionField::Port,
                &port,
                port_placeholder.to_string(),
                layout.content_width,
                cx,
            ))
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
        let username_placeholder =
            if protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp {
                "Administrator".to_string()
            } else {
                self.i18n.t("modals.new_connection.remote_desktop_username")
            };
        let username_field = self.render_connection_field(
            self.i18n.t("modals.new_connection.remote_desktop_username"),
            &username,
            username_placeholder,
            NewConnectionField::Username,
            false,
            cx,
        );
        let password_field = self.render_connection_secret_field(
            self.i18n.t("ssh.form.password"),
            if empty_password {
                String::new()
            } else if keeps_saved_password {
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
        );
        let credentials_row = if layout.narrow_content {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(username_field)
                .child(password_field)
        } else {
            div()
                .w_full()
                .flex()
                .flex_row()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(username_field))
                .child(div().flex_1().min_w_0().child(password_field))
        };
        let empty_switch = self.render_connection_switch_row(
            "remote-desktop-empty-password",
            self.i18n.t("ssh.form.use_empty_password"),
            empty_password,
            false,
            |form| {
                form.empty_password = !form.empty_password;
                if form.empty_password {
                    form.field_focused = false;
                    form.password.zeroize();
                }
            },
            cx,
        );
        let password_switches = if !empty_password && !layout.compact_content {
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(18.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.render_connection_switch_row(
                            "remote-desktop-save-password",
                            self.i18n.t("ssh.form.save_password"),
                            save_password,
                            false,
                            |form| form.save_password = !form.save_password,
                            cx,
                        )),
                )
                .child(div().flex_1().min_w_0().child(empty_switch))
        } else {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(!empty_password, |col| {
                    col.child(self.render_connection_switch_row(
                        "remote-desktop-save-password",
                        self.i18n.t("ssh.form.save_password"),
                        save_password,
                        false,
                        |form| form.save_password = !form.save_password,
                        cx,
                    ))
                    .child(self.form_hairline())
                })
                .child(empty_switch)
        };
        let authentication = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(credentials_row)
            .child(password_switches)
            .into_any_element();

        let gateway_name = ssh_gateway_connection_id.as_deref().and_then(|id| {
            self.environment
                .connections
                .iter()
                .find(|connection| connection.id == id)
                .map(|connection| connection.name.clone())
        });
        let route_highlighted = ssh_gateway_connection_id.is_some()
            || (protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp
                && upstream_proxy_policy != NewConnectionUpstreamProxyPolicy::Direct);
        let route_summary = if let Some(name) = gateway_name {
            name
        } else if ssh_gateway_connection_id.is_some() {
            self.i18n
                .t("modals.new_connection.remote_desktop_ssh_gateway_missing")
        } else if protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp
            && upstream_proxy_policy != NewConnectionUpstreamProxyPolicy::Direct
        {
            match upstream_proxy_policy {
                NewConnectionUpstreamProxyPolicy::UseGlobal => {
                    self.i18n.t("modals.upstream_proxy.use_global")
                }
                NewConnectionUpstreamProxyPolicy::Custom => {
                    self.i18n.t("modals.upstream_proxy.custom")
                }
                NewConnectionUpstreamProxyPolicy::Direct => self
                    .i18n
                    .t("modals.new_connection.remote_desktop_ssh_gateway_direct"),
            }
        } else {
            self.i18n
                .t("modals.new_connection.remote_desktop_ssh_gateway_direct")
        };
        let vnc_customized = vnc_options != Default::default();
        let vnc_summary = if vnc_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };
        let features_customized = session_options
            != oxideterm_remote_desktop::RemoteDesktopSessionOptions {
                vnc: vnc_options,
                ..Default::default()
            };
        let features_summary = if features_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };
        let pre_connect_customized = !pre_connect_command.trim().is_empty();
        let terminal_summary = if pre_connect_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };
        let notes_trimmed = notes.trim();
        let notes_highlighted = !notes_trimmed.is_empty() || appearance_customized;
        let notes_summary = if !notes_trimmed.is_empty() {
            notes_trimmed
                .lines()
                .next()
                .unwrap_or(notes_trimmed)
                .to_string()
        } else if appearance_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .when(active_drawer.is_none(), |col| {
                col.child({
                    let available = match protocol {
                        oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp => {
                            self.environment.rdp_available
                        }
                        oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc => {
                            self.environment.vnc_available
                        }
                    };
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(self.render_connection_hint_with_color(
                            if available {
                                self.i18n.t("remote_desktop.plugin_ready")
                            } else {
                                self.plugin_requirement_message(
                                    &format!(
                                        "com.oxideterm.remote-desktop.{}",
                                        protocol.provider_id()
                                    ),
                                    &protocol.provider_id().to_uppercase(),
                                    cx,
                                )
                            },
                            if available {
                                self.tokens.ui.success
                            } else {
                                self.tokens.ui.warning
                            },
                        ))
                        .when(!available, |row| {
                            row.child(self.workspace_toolbar_action_button(
                                self.i18n.t("remote_desktop.manage_plugin"),
                                None,
                                ToolbarButtonOptions::default(),
                                cx.listener(move |this, _event, window, cx| {
                                    this.close_new_connection_form(window, cx);
                                    this.request(
                                        FormAction::OpenRemoteDesktopPlugin(protocol),
                                        Some(window),
                                        cx,
                                    );
                                }),
                            ))
                        })
                })
            })
            .child({
                use crate::form_state::ConnectionFormDrawer;
                match active_drawer {
                    Some(ConnectionFormDrawer::Route) => {
                        let mut gateway = div().flex().flex_col().gap_4().child(
                            self.render_remote_desktop_ssh_gateway_select(
                                ssh_gateway_connection_id.as_deref(),
                                cx,
                            ),
                        );
                        if protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp {
                            gateway = gateway.child(
                                self.render_connection_hint(
                                    self.i18n
                                        .t("modals.new_connection.remote_desktop_proxy_route_hint"),
                                ),
                            );
                            gateway = gateway
                                .child(self.form_hairline())
                                .child(self.render_upstream_proxy_policy_section(false, cx));
                        }
                        gateway.into_any_element()
                    }
                    Some(ConnectionFormDrawer::VncPreferences)
                        if protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc =>
                    {
                        self.render_vnc_connection_preferences(cx)
                    }
                    Some(ConnectionFormDrawer::RemoteFeatures) => {
                        self.render_remote_desktop_features(protocol, &capabilities, cx)
                    }
                    Some(ConnectionFormDrawer::Terminal) => {
                        self.render_connection_pre_connect_command(cx)
                    }
                    Some(ConnectionFormDrawer::Notes) => div()
                        .flex()
                        .flex_col()
                        .gap(px(self.tokens.metrics.modal_section_gap))
                        .child(self.render_connection_appearance_options(cx))
                        .child(self.form_hairline())
                        .child(self.render_connection_notes_fields(&notes, cx))
                        .into_any_element(),
                    _ => {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(basic)
                            .child(self.form_hairline())
                            .child(authentication)
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(self.form_hairline())
                                    .child(
                                        self.render_connection_disclosure_row(
                                            "rdp-summary-Route",
                                            LucideIcon::Network,
                                            self.i18n.t(
                                                "modals.new_connection.remote_desktop_ssh_gateway",
                                            ),
                                            route_summary,
                                            route_highlighted,
                                            ConnectionFormDrawer::Route,
                                            cx,
                                        ),
                                    )
                                    .when(
                                        protocol
                                            == oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc,
                                        |rows| {
                                            rows.child(self.form_hairline())
                                                .child(self.render_connection_disclosure_row(
                                                "rdp-summary-VncPreferences",
                                                LucideIcon::Settings,
                                                self.i18n.t(
                                                    "modals.new_connection.vnc_preferences_title",
                                                ),
                                                vnc_summary,
                                                vnc_customized,
                                                ConnectionFormDrawer::VncPreferences,
                                                cx,
                                            ))
                                        },
                                    )
                                    .child(self.form_hairline())
                                    .child(self.render_connection_disclosure_row(
                                        "rdp-summary-RemoteFeatures",
                                        LucideIcon::Shield,
                                        self.i18n.t(
                                            "modals.new_connection.remote_desktop_features_title",
                                        ),
                                        features_summary,
                                        features_customized,
                                        ConnectionFormDrawer::RemoteFeatures,
                                        cx,
                                    ))
                                    .child(self.form_hairline())
                                    .child(self.render_connection_disclosure_row(
                                        "rdp-summary-Terminal",
                                        LucideIcon::Terminal,
                                        self.i18n.t("ssh.form.terminal_options"),
                                        terminal_summary,
                                        pre_connect_customized,
                                        ConnectionFormDrawer::Terminal,
                                        cx,
                                    ))
                                    .child(self.form_hairline())
                                    .child(self.render_connection_disclosure_row(
                                        "rdp-summary-Notes",
                                        LucideIcon::Sparkles,
                                        self.i18n.t("ssh.form.appearance"),
                                        notes_summary,
                                        notes_highlighted,
                                        ConnectionFormDrawer::Notes,
                                        cx,
                                    )),
                            )
                            .into_any_element()
                    }
                }
            })
            .into_any_element()
    }

    fn render_vnc_connection_preferences(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_security_policy",
                "modals.new_connection.vnc_security_policy_hint",
                VNC_SECURITY_PREFERENCES,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_session_mode",
                "modals.new_connection.vnc_session_mode_hint",
                VNC_SESSION_MODE_PREFERENCES,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_vnc_preference_group(
                "modals.new_connection.vnc_image_quality",
                "modals.new_connection.vnc_image_quality_hint",
                VNC_IMAGE_QUALITY_PREFERENCES,
                cx,
            ))
            .child(self.form_hairline())
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
                self.render_sunken_segmented_tab(
                    SharedString::from(format!("vnc-preference-{title_key}-{index}")),
                    self.i18n.t(label_key),
                    remote_desktop_vnc_preference_selected(&current, preference),
                )
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
                self.render_sunken_segmented_track().children(options),
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
            .gap(px(12.0))
            .child(self.render_remote_desktop_feature_group(
                "modals.new_connection.remote_desktop_clipboard_group",
                REMOTE_DESKTOP_CLIPBOARD_FEATURES,
                capabilities,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_remote_desktop_feature_group(
                "modals.new_connection.remote_desktop_audio_group",
                REMOTE_DESKTOP_AUDIO_FEATURES,
                capabilities,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_remote_desktop_feature_group(
                "modals.new_connection.remote_desktop_display_group",
                REMOTE_DESKTOP_DISPLAY_FEATURES,
                capabilities,
                cx,
            ))
            .when(
                protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
                |features| {
                    features
                        .child(self.form_hairline())
                        .child(self.render_rdp_network_profile(cx))
                        .child(self.form_hairline())
                        .child(self.render_remote_desktop_feature_group(
                            "modals.new_connection.remote_desktop_compatibility_group",
                            RDP_COMPATIBILITY_FEATURES,
                            capabilities,
                            cx,
                        ))
                },
            )
            .into_any_element()
    }

    fn render_rdp_network_profile(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected_profile = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.remote_desktop_session_options.rdp.network_profile)
            .unwrap_or_default();
        let options =
            RDP_NETWORK_PROFILES
                .iter()
                .enumerate()
                .map(|(index, (network_profile, label_key))| {
                    let network_profile = *network_profile;
                    self.render_sunken_segmented_tab(
                        SharedString::from(format!("rdp-network-profile-{index}")),
                        self.i18n.t(label_key),
                        selected_profile == network_profile,
                    )
                    .whitespace_normal()
                    .text_align(gpui::TextAlign::Center)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.remote_desktop_session_options.rdp.network_profile =
                                        network_profile;
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
                self.i18n
                    .t("modals.new_connection.remote_desktop_rdp_network_profile"),
                self.render_sunken_segmented_track().children(options),
            ))
            .child(
                self.render_connection_hint(
                    self.i18n
                        .t("modals.new_connection.remote_desktop_rdp_network_profile_hint"),
                ),
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
        let mut list = div().flex().flex_col().gap(px(2.0));
        for (index, (feature, label_key, hint_key)) in features.iter().enumerate() {
            if index > 0 {
                list = list.child(self.form_hairline());
            }
            list = list.child(self.render_remote_desktop_feature_row(
                self.i18n.t(label_key),
                self.i18n.t(hint_key),
                remote_desktop_feature_supported(capabilities, *feature),
                *feature,
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .px(px(2.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(title_key)),
            )
            .child(list)
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
        let theme = self.tokens.ui;
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
        let row_id = SharedString::from(format!("remote-desktop-feature-{feature:?}"));
        let switch_id = SharedString::from(format!("track-remote-desktop-feature-{feature:?}"));

        div()
            .id(row_id)
            .w_full()
            .py(px(6.0))
            .px(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .cursor(if supported {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .when(supported, |row| {
                row.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x45)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(if supported {
                                theme.text
                            } else {
                                theme.text_muted
                            }))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(if supported {
                                theme.text_muted
                            } else {
                                theme.warning
                            }))
                            .child(hint),
                    ),
            )
            .child(switch_with(
                &self.tokens,
                switch_id,
                "",
                selected,
                SwitchOptions {
                    disabled: !supported,
                    ..SwitchOptions::default()
                },
            ))
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
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_telnet_form_branch(
        &self,
        active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((
            host,
            port,
            profile_name,
            notes,
            upstream_proxy_policy,
            terminal_customized,
            appearance_customized,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.host.clone(),
                form.port.clone(),
                form.telnet_profile_name.clone(),
                form.notes.clone(),
                form.upstream_proxy_policy,
                form.terminal != Default::default() || !form.pre_connect_command.trim().is_empty(),
                !form.icon.trim().is_empty()
                    || !form.color.trim().is_empty()
                    || !form.icon_background_color.trim().is_empty(),
            )
        })
        else {
            return div().into_any_element();
        };
        let telnet_port_invalid = !port.trim().is_empty() && port.trim().parse::<u16>().is_err();
        let route_highlighted = upstream_proxy_policy != NewConnectionUpstreamProxyPolicy::Direct;
        let route_summary = match upstream_proxy_policy {
            NewConnectionUpstreamProxyPolicy::UseGlobal => {
                self.i18n.t("modals.upstream_proxy.use_global")
            }
            NewConnectionUpstreamProxyPolicy::Direct => self.i18n.t("modals.upstream_proxy.direct"),
            NewConnectionUpstreamProxyPolicy::Custom => self.i18n.t("modals.upstream_proxy.custom"),
        };
        let terminal_summary = if terminal_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };
        let notes_trimmed = notes.trim();
        let notes_highlighted = !notes_trimmed.is_empty() || appearance_customized;
        let notes_summary = if !notes_trimmed.is_empty() {
            notes_trimmed
                .lines()
                .next()
                .unwrap_or(notes_trimmed)
                .to_string()
        } else if appearance_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };

        use crate::form_state::ConnectionFormDrawer;
        let basic = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                self.render_identity_row(
                    self.i18n.t("modals.new_connection.telnet_profile_name"),
                    &profile_name,
                    self.i18n
                        .t("modals.new_connection.telnet_profile_name_placeholder"),
                    NewConnectionField::TelnetProfileName,
                    None,
                    true,
                    layout,
                    cx,
                ),
            )
            .child(self.render_unified_host_port_bar(
                self.i18n.t("modals.new_connection.telnet_host"),
                NewConnectionField::Host,
                &host,
                self.i18n.t("modals.new_connection.telnet_host_placeholder"),
                self.i18n.t("modals.new_connection.telnet_port"),
                NewConnectionField::Port,
                &port,
                TELNET_DEFAULT_PORT_TEXT.to_string(),
                layout.content_width,
                cx,
            ))
            .when(telnet_port_invalid, |section| {
                section.child(self.render_connection_hint_with_color(
                    self.i18n.t("modals.new_connection.telnet_invalid_port"),
                    self.tokens.ui.error,
                ))
            })
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(self.form_hairline())
                    .child(self.render_connection_disclosure_row(
                        "telnet-summary-Route",
                        LucideIcon::Network,
                        self.i18n.t("ssh.form.connection_route"),
                        route_summary,
                        route_highlighted,
                        ConnectionFormDrawer::Route,
                        cx,
                    ))
                    .child(self.form_hairline())
                    .child(self.render_connection_disclosure_row(
                        "telnet-summary-Terminal",
                        LucideIcon::Terminal,
                        self.i18n.t("ssh.form.terminal_options"),
                        terminal_summary,
                        terminal_customized,
                        ConnectionFormDrawer::Terminal,
                        cx,
                    ))
                    .child(self.form_hairline())
                    .child(self.render_connection_disclosure_row(
                        "telnet-summary-Notes",
                        LucideIcon::Sparkles,
                        self.i18n.t("ssh.form.appearance"),
                        notes_summary,
                        notes_highlighted,
                        ConnectionFormDrawer::Notes,
                        cx,
                    )),
            )
            .into_any_element();
        match active_drawer {
            Some(ConnectionFormDrawer::Route) => {
                self.render_upstream_proxy_policy_section(false, cx)
            }
            Some(ConnectionFormDrawer::Terminal) => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_terminal_options(layout.narrow_content, cx))
                .child(self.form_hairline())
                .child(self.render_connection_pre_connect_command(cx))
                .into_any_element(),
            Some(ConnectionFormDrawer::Notes) => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_appearance_options(cx))
                .child(self.form_hairline())
                .child(self.render_connection_notes_fields(&notes, cx))
                .into_any_element(),
            _ => basic,
        }
    }

    pub(in crate::views) fn refresh_serial_ports(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.serial_ports_loading = true;
                form.error = None;
            }
        });
        cx.notify();

        let result = (self.serial_ports)();
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

    pub(super) fn render_serial_form_branch(
        &self,
        active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((
            ports,
            baud_rate,
            data_bits,
            stop_bits,
            parity,
            flow_control,
            profile_name,
            notes,
            pre_connect_command,
            appearance_customized,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.serial_ports.clone(),
                form.serial_baud_rate.clone(),
                form.serial_data_bits,
                form.serial_stop_bits,
                form.serial_parity,
                form.serial_flow_control,
                form.serial_profile_name.clone(),
                form.notes.clone(),
                form.pre_connect_command.clone(),
                !form.icon.trim().is_empty()
                    || !form.color.trim().is_empty()
                    || !form.icon_background_color.trim().is_empty(),
            )
        })
        else {
            return div().into_any_element();
        };
        let serial_baud_rate_invalid = !baud_rate.trim().is_empty()
            && !baud_rate.trim().parse::<u32>().is_ok_and(|baud| baud > 0);
        let baud_field = div()
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
            });
        let data_bits_field = self.render_serial_u8_select(
            self.i18n.t("modals.new_connection.serial_data_bits"),
            NewConnectionSelect::SerialDataBits,
            &[(5, "5"), (6, "6"), (7, "7"), (8, "8")],
            data_bits,
            cx,
        );
        let stop_bits_field = self.render_serial_u8_select(
            self.i18n.t("modals.new_connection.serial_stop_bits"),
            NewConnectionSelect::SerialStopBits,
            &[(1, "1"), (2, "2")],
            stop_bits,
            cx,
        );
        let parity_field = self.render_serial_parity_select(parity, cx);
        let flow_field = self.render_serial_flow_select(flow_control, cx);

        let baud_row = if layout.narrow_content {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(baud_field)
                .child(data_bits_field)
        } else {
            div()
                .w_full()
                .flex()
                .flex_row()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(baud_field))
                .child(div().flex_1().min_w_0().child(data_bits_field))
        };
        let framing_row = if layout.narrow_content {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(stop_bits_field)
                .child(parity_field)
                .child(flow_field)
        } else {
            div()
                .w_full()
                .flex()
                .flex_row()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(stop_bits_field))
                .child(div().flex_1().min_w_0().child(parity_field))
                .child(div().flex_1().min_w_0().child(flow_field))
        };
        let parameters = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_serial_port_field(&ports, cx))
            .child(baud_row)
            .child(framing_row)
            .into_any_element();
        let pre_connect_customized = !pre_connect_command.trim().is_empty();
        let terminal_summary = if pre_connect_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };
        let notes_trimmed = notes.trim();
        let notes_highlighted = !notes_trimmed.is_empty() || appearance_customized;
        let notes_summary = if !notes_trimmed.is_empty() {
            notes_trimmed
                .lines()
                .next()
                .unwrap_or(notes_trimmed)
                .to_string()
        } else if appearance_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };

        use crate::form_state::ConnectionFormDrawer;
        match active_drawer {
            Some(ConnectionFormDrawer::Terminal) => self.render_connection_pre_connect_command(cx),
            Some(ConnectionFormDrawer::Notes) => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_appearance_options(cx))
                .child(self.form_hairline())
                .child(self.render_connection_notes_fields(&notes, cx))
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    self.render_identity_row(
                        self.i18n.t("modals.new_connection.serial_profile_name"),
                        &profile_name,
                        self.i18n
                            .t("modals.new_connection.serial_profile_name_placeholder"),
                        NewConnectionField::SerialProfileName,
                        None,
                        true,
                        layout,
                        cx,
                    ),
                )
                .child(self.form_hairline())
                .child(parameters)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(self.form_hairline())
                        .child(self.render_connection_disclosure_row(
                            "serial-summary-Terminal",
                            LucideIcon::Terminal,
                            self.i18n.t("ssh.form.terminal_options"),
                            terminal_summary,
                            pre_connect_customized,
                            ConnectionFormDrawer::Terminal,
                            cx,
                        ))
                        .child(self.form_hairline())
                        .child(self.render_connection_disclosure_row(
                            "serial-summary-Notes",
                            LucideIcon::Sparkles,
                            self.i18n.t("ssh.form.appearance"),
                            notes_summary,
                            notes_highlighted,
                            ConnectionFormDrawer::Notes,
                            cx,
                        )),
                )
                .into_any_element(),
        }
    }

    fn render_serial_port_field(
        &self,
        ports: &[crate::form_model::SerialPortInfo],
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

    pub(super) fn render_new_connection_select_control(
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
                        this.request(FormAction::ClearComposition, None, cx);
                        this.open_new_connection_select_from_pointer(select_id, cx);
                        this.request(FormAction::FocusForm, Some(window), cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
            });

        self.track_new_connection_select_anchor(select_id, trigger, cx)
    }

    pub(super) fn render_connection_checkbox_with_warning(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        toggle: fn(&mut NewConnectionForm),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(self.render_connection_checkbox(
                trigger_id,
                self.i18n.t(label_key),
                checked,
                toggle,
                cx,
            ))
            .child(self.render_connection_help_icon_with_icon(
                trigger_id,
                tooltip_id,
                hint_key,
                LucideIcon::AlertCircle,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_connection_label_with_help(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        hint_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(self.i18n.t(label_key))
            .child(self.render_connection_help_icon(trigger_id, tooltip_id, hint_key, cx))
            .into_any_element()
    }

    fn render_connection_help_icon(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        hint_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_connection_help_icon_with_icon(
            trigger_id,
            tooltip_id,
            hint_key,
            LucideIcon::Info,
            cx,
        )
    }

    fn render_connection_help_icon_with_icon(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        hint_key: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let icon_color = match icon {
            LucideIcon::AlertCircle => theme.warning,
            _ => theme.text_muted,
        };
        // All advanced SSH explanations share one existing tooltip interaction
        // so dense forms do not alternate between inline and hidden help text.
        div()
            .id(trigger_id)
            .size(px(18.0))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x80)))
            .child(Self::render_lucide_icon(icon, 13.0, rgb(icon_color)))
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
            }))
            .into_any_element()
    }

    /// Renders the local gate shared by every connection type. The value is
    /// optional and runs on this machine, so one label and one hint are enough.
    pub(super) fn render_connection_pre_connect_command(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(command) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.pre_connect_command.clone())
        else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(self.render_connection_field(
                self.i18n.t("ssh.form.pre_connect_command"),
                &command,
                self.i18n.t("ssh.form.pre_connect_command_placeholder"),
                NewConnectionField::PreConnectCommand,
                false,
                cx,
            ))
            .child(self.render_connection_hint(self.i18n.t("ssh.form.pre_connect_command_hint")))
            .into_any_element()
    }

    pub(super) fn render_connection_terminal_options(
        &self,
        single_column: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Saved host controls are optional overrides so application defaults
        // continue to govern legacy records and temporary local terminals.
        let Some(terminal) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.terminal.clone())
        else {
            return div().into_any_element();
        };
        let telnet_line_endings = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .filter(|form| form.transport == NewConnectionTransport::Telnet)
            .map(|form| {
                (
                    form.telnet_input_line_ending,
                    form.telnet_output_line_ending,
                )
            });
        // BLE keeps its byte-stream runtime controls (send/display modes,
        // line endings, local echo) inside the shared terminal behavior
        // section; only MTU and the write mode stay in transport options.
        let ble_runtime_controls = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .filter(|form| form.transport == NewConnectionTransport::Ble)
            .map(|form| {
                (
                    form.ble.input_line_ending,
                    form.ble.output_line_ending,
                    form.ble.send_mode,
                    form.ble.display_mode,
                    form.ble.local_echo,
                )
            });
        let application_defaults = &self.environment.terminal;
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
        let inherited_session_log_policy_label = if application_defaults.session_log.automatic {
            self.i18n.t("ssh.form.terminal_session_log_automatic")
        } else {
            self.i18n.t("ssh.form.terminal_session_log_manual")
        };
        let session_log_policy_label = match terminal.session_log_policy {
            ConnectionTerminalSessionLogPolicy::Inherit => {
                inherited_label(&inherited_session_log_policy_label)
            }
            ConnectionTerminalSessionLogPolicy::Automatic => {
                self.i18n.t("ssh.form.terminal_session_log_automatic")
            }
            ConnectionTerminalSessionLogPolicy::Manual => {
                self.i18n.t("ssh.form.terminal_session_log_manual")
            }
            ConnectionTerminalSessionLogPolicy::Disabled => {
                self.i18n.t("ssh.form.terminal_session_log_disabled")
            }
        };
        let inherited_bell_style_label = match application_defaults.bell_style {
            oxideterm_settings::TerminalBellStyle::Off => {
                self.i18n.t("settings_view.terminal.bell_style_off")
            }
            oxideterm_settings::TerminalBellStyle::SoftFlash => {
                self.i18n.t("settings_view.terminal.bell_style_soft_flash")
            }
            oxideterm_settings::TerminalBellStyle::EdgeGlow => {
                self.i18n.t("settings_view.terminal.bell_style_edge_glow")
            }
            oxideterm_settings::TerminalBellStyle::CursorPulse => self
                .i18n
                .t("settings_view.terminal.bell_style_cursor_pulse"),
            oxideterm_settings::TerminalBellStyle::HeaderStrip => self
                .i18n
                .t("settings_view.terminal.bell_style_header_strip"),
        };
        let bell_policy_label = match terminal.bell_policy {
            ConnectionTerminalBellPolicy::Inherit => inherited_label(&inherited_bell_style_label),
            ConnectionTerminalBellPolicy::Silent => self.i18n.t("ssh.form.terminal_bell_silent"),
            ConnectionTerminalBellPolicy::TabHighlight => {
                self.i18n.t("ssh.form.terminal_bell_tab_highlight")
            }
        };

        let mut controls: Vec<AnyElement> = vec![
            form_field(
                &self.tokens,
                self.i18n.t("settings_view.terminal.encoding"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::TerminalEncoding,
                    encoding_label,
                    false,
                    false,
                    cx,
                ),
            ),
            form_field(
                &self.tokens,
                self.i18n.t("settings_view.terminal.backspace_sequence"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::TerminalBackspaceSequence,
                    backspace_label,
                    false,
                    false,
                    cx,
                ),
            ),
            form_field(
                &self.tokens,
                self.i18n.t("settings_view.terminal.delete_sequence"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::TerminalDeleteSequence,
                    delete_label,
                    false,
                    false,
                    cx,
                ),
            ),
        ];
        if let Some((input, output)) = telnet_line_endings {
            for (select, label, value) in [
                (
                    NewConnectionSelect::TelnetInputLineEnding,
                    "terminal.serial_control.line_ending",
                    input,
                ),
                (
                    NewConnectionSelect::TelnetOutputLineEnding,
                    "terminal.serial_control.output_line_ending",
                    output,
                ),
            ] {
                controls.push(form_field(
                    &self.tokens,
                    self.i18n.t(label),
                    self.render_new_connection_select_control(
                        select,
                        self.connection_line_ending_label(value),
                        false,
                        false,
                        cx,
                    ),
                ));
            }
        }
        controls.extend([
            form_field(
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
            ),
            form_field(
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
            ),
            form_field(
                &self.tokens,
                self.i18n.t("ssh.form.terminal_session_log"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::TerminalSessionLogPolicy,
                    session_log_policy_label,
                    false,
                    false,
                    cx,
                ),
            ),
            form_field(
                &self.tokens,
                self.i18n.t("ssh.form.terminal_bell_policy"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::TerminalBellPolicy,
                    bell_policy_label,
                    false,
                    false,
                    cx,
                ),
            ),
        ]);
        let mut ble_local_echo_row: Option<AnyElement> = None;
        if let Some((input, output, send_mode, display_mode, local_echo)) = ble_runtime_controls {
            controls.extend([
                form_field(
                    &self.tokens,
                    self.i18n.t("terminal.serial_control.send_mode"),
                    self.render_new_connection_select_control(
                        NewConnectionSelect::BleSendMode,
                        self.connection_send_mode_label(send_mode),
                        false,
                        false,
                        cx,
                    ),
                ),
                form_field(
                    &self.tokens,
                    self.i18n.t("terminal.serial_control.display_mode"),
                    self.render_new_connection_select_control(
                        NewConnectionSelect::BleDisplayMode,
                        self.connection_display_mode_label(display_mode),
                        false,
                        false,
                        cx,
                    ),
                ),
                form_field(
                    &self.tokens,
                    self.i18n.t("terminal.serial_control.line_ending"),
                    self.render_new_connection_select_control(
                        NewConnectionSelect::BleInputLineEnding,
                        self.connection_line_ending_label(input),
                        false,
                        false,
                        cx,
                    ),
                ),
                form_field(
                    &self.tokens,
                    self.i18n.t("terminal.serial_control.output_line_ending"),
                    self.render_new_connection_select_control(
                        NewConnectionSelect::BleOutputLineEnding,
                        self.connection_line_ending_label(output),
                        false,
                        false,
                        cx,
                    ),
                ),
            ]);
            ble_local_echo_row = Some(self.render_connection_switch_row(
                "ble-local-echo",
                self.i18n.t("terminal.serial_control.local_echo"),
                local_echo,
                false,
                |form| form.ble.local_echo = !form.ble.local_echo,
                cx,
            ));
        }

        // Lay out select controls in balanced 3-column rows on desktop and fold
        // cleanly to a single column on narrow viewports so values never clip.
        let mut grid = div().flex().flex_col().gap(px(12.0));
        if single_column {
            for control in controls {
                grid = grid.child(div().w_full().min_w_0().child(control));
            }
        } else {
            let mut iter = controls.into_iter();
            while let Some(first) = iter.next() {
                let second = iter.next();
                let third = iter.next();
                grid = grid.child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap(px(12.0))
                        .child(div().flex_1().min_w_0().child(first))
                        .child(if let Some(second) = second {
                            div().flex_1().min_w_0().child(second)
                        } else {
                            div().flex_1().min_w_0()
                        })
                        .child(if let Some(third) = third {
                            div().flex_1().min_w_0().child(third)
                        } else {
                            div().flex_1().min_w_0()
                        }),
                );
            }
        }
        if let Some(row) = ble_local_echo_row {
            grid = grid.child(self.form_hairline()).child(row);
        }
        grid.into_any_element()
    }

    pub(super) fn render_connection_appearance_options(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Session asset appearance is shared by every form that can persist a
        // connection.
        let Some((icon, color, icon_background_color, icon_picker_expanded, transport)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.icon.clone(),
                    form.color.clone(),
                    form.icon_background_color.clone(),
                    form.icon_picker_expanded,
                    form.transport,
                )
            })
        else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_edit_icon_field(
                &icon,
                &color,
                &icon_background_color,
                icon_picker_expanded,
                transport == NewConnectionTransport::Ssh,
                default_icon_for_transport(transport),
                cx,
            ))
            .child(
                // The two asset colors form one responsive pair: share a row
                // when the modal has room and wrap together without compressing
                // either input below a usable width.
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_ICON_COLOR_CONTROL_MIN_WIDTH))
                            .child(self.render_edit_color_field(
                                self.i18n.t("sessionManager.edit_properties.icon_color"),
                                &color,
                                NewConnectionField::Color,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(CONNECTION_ICON_COLOR_CONTROL_MIN_WIDTH))
                            .child(
                                self.render_edit_color_field(
                                    self.i18n
                                        .t("sessionManager.edit_properties.icon_background_color"),
                                    &icon_background_color,
                                    NewConnectionField::IconBackgroundColor,
                                    cx,
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_open_sftp_switch(&self, open_sftp_on_connect: bool, cx: &mut Context<Self>) -> AnyElement {
        self.render_connection_switch_row_with_help(
                "new-connection-open-sftp-help",
                "new-connection-open-sftp",
                "ssh.form.open_sftp_on_connect",
                "ssh.form.open_sftp_on_connect_hint",
                open_sftp_on_connect,
                false,
                |form| {
                    form.open_sftp_on_connect = !form.open_sftp_on_connect;
                },
                cx,
            )
    }

    pub(super) fn render_connection_ssh_options(
        &self,
        agent_forwarding: bool,
        legacy_ssh_compatibility: bool,
        content_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.environment.browser.is_some() {
            let checked = self.connection_form_state(cx).form.as_ref().is_some_and(|form| form.open_sftp_on_connect);
            return self.render_open_sftp_switch(checked, cx);
        }
        let Some((
            connect_timeout_seconds_text,
            keep_alive_interval_seconds_text,
            keep_alive_count_max_text,
            x11_forwarding,
            ssh_channel_strategy,
            dedicated_new_terminal_connection,
            open_sftp_on_connect,
            gssapi_enabled,
            gssapi_server_identity,
            gssapi_delegate_credentials,
            gssapi_credentials_available,
            ssh_algorithms_custom_count,
        )) = self.connection_form_state(cx).form.as_ref().map(|form| {
            (
                form.connect_timeout_seconds_text.clone(),
                form.keep_alive_interval_seconds_text.clone(),
                form.keep_alive_count_max_text.clone(),
                form.x11_forwarding,
                form.ssh_channel_strategy,
                form.dedicated_new_terminal_connection,
                form.open_sftp_on_connect,
                form.gssapi_enabled,
                form.gssapi_server_identity.clone(),
                form.gssapi_delegate_credentials,
                form.gssapi_credentials_available,
                [
                    !form.ssh_algorithms.kex.is_empty(),
                    !form.ssh_algorithms.host_key.is_empty(),
                    !form.ssh_algorithms.cipher.is_empty(),
                    !form.ssh_algorithms.mac.is_empty(),
                    !form.ssh_algorithms.compression.is_empty(),
                ]
                .into_iter()
                .filter(|customized| *customized)
                .count(),
            )
        })
        else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let x11_mode_label = match x11_forwarding.mode {
            ConnectionX11ForwardingMode::Untrusted => self.i18n.t("ssh.form.x11_mode_untrusted"),
            ConnectionX11ForwardingMode::Trusted => self.i18n.t("ssh.form.x11_mode_trusted"),
        };
        let x11_timeout_label = match x11_forwarding.untrusted_timeout_seconds {
            300 => self.i18n.t("ssh.form.x11_timeout_5_minutes"),
            1_200 => self.i18n.t("ssh.form.x11_timeout_20_minutes"),
            3_600 => self.i18n.t("ssh.form.x11_timeout_1_hour"),
            _ => self.i18n.t("ssh.form.x11_timeout_none"),
        };
        let stacked_select = content_width < 440.0;
        let inline_select_width = (content_width * 0.36).clamp(152.0, 220.0).round();
        let render_inline_select_row =
            |select_id: NewConnectionSelect,
             trigger_id: &'static str,
             tooltip_id: &'static str,
             label_key: &'static str,
             hint_key: &'static str,
             value: String,
             cx: &mut Context<Self>| {
                let trigger = self
                    .new_connection_select_trigger(select_id, value, false, false, cx)
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
                            this.request(FormAction::ClearComposition, None, cx);
                            this.open_new_connection_select_from_pointer(select_id, cx);
                            this.request(FormAction::FocusForm, Some(window), cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    );
                let label = div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(self.render_connection_label_with_help(
                        trigger_id, tooltip_id, label_key, hint_key, cx,
                    ));
                let select = self.track_new_connection_select_anchor(select_id, trigger, cx);
                if stacked_select {
                    div()
                        .w_full()
                        .py(px(6.0))
                        .pl(px(14.0))
                        .pr(px(2.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(label)
                        .child(div().w_full().child(select))
                } else {
                    div()
                        .w_full()
                        .min_h(px(44.0))
                        .py(px(6.0))
                        .pl(px(14.0))
                        .pr(px(2.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(label)
                        .child(div().w(px(inline_select_width)).flex_none().child(select))
                }
            };

        let timeout_row = div()
            .w_full()
            .min_h(px(44.0))
            .py(px(6.0))
            .px(px(2.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(self.render_connection_label_with_help(
                        "new-connection-connect-timeout-help",
                        "new-connection-connect-timeout",
                        "ssh.form.connect_timeout",
                        "ssh.form.connect_timeout_hint",
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(84.0))
                            .child(self.render_connection_centered_mono_input(
                                &connect_timeout_seconds_text,
                                crate::form_model::DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS.to_string(),
                                NewConnectionField::ConnectTimeoutSeconds,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(SharedString::from(
                                self.tokens.metrics.markdown_code_font_family,
                            ))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child("s"),
                    ),
            );

        let keep_alive_interval_row =
            div()
                .w_full()
                .min_h(px(44.0))
                .py(px(6.0))
                .px(px(2.0))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text))
                        .child(self.render_connection_label_with_help(
                            "new-connection-keep-alive-interval-help",
                            "new-connection-keep-alive-interval",
                            "ssh.form.keep_alive_interval",
                            "ssh.form.keep_alive_interval_hint",
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div().w(px(84.0)).child(
                                self.render_connection_centered_mono_input(
                                    &keep_alive_interval_seconds_text,
                                    crate::form_model::DEFAULT_SSH_KEEP_ALIVE_INTERVAL_SECONDS
                                        .to_string(),
                                    NewConnectionField::KeepAliveIntervalSeconds,
                                    cx,
                                ),
                            ),
                        )
                        .child(
                            div()
                                .font_family(SharedString::from(
                                    self.tokens.metrics.markdown_code_font_family,
                                ))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child("s"),
                        ),
                );

        let keep_alive_count_max_row =
            div()
                .w_full()
                .min_h(px(44.0))
                .py(px(6.0))
                .px(px(2.0))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text))
                        .child(self.render_connection_label_with_help(
                            "new-connection-keep-alive-count-max-help",
                            "new-connection-keep-alive-count-max",
                            "ssh.form.keep_alive_count_max",
                            "ssh.form.keep_alive_count_max_hint",
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().w(px(84.0)).child(
                            self.render_connection_centered_mono_input(
                                &keep_alive_count_max_text,
                                crate::form_model::DEFAULT_SSH_KEEP_ALIVE_COUNT_MAX.to_string(),
                                NewConnectionField::KeepAliveCountMax,
                                cx,
                            ),
                        )),
                );

        let algorithms_summary = if ssh_algorithms_custom_count > 0 {
            self.i18n
                .t("ssh.form.ssh_algorithms_custom_count")
                .replace("{{count}}", &ssh_algorithms_custom_count.to_string())
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_follow_preset")
        };

        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(timeout_row)
            .child(self.form_hairline())
            .child(keep_alive_interval_row)
            .child(self.form_hairline())
            .child(keep_alive_count_max_row)
            .child(self.form_hairline())
            .child(self.render_connection_switch_row_with_help(
                "new-connection-single-channel-help",
                "new-connection-single-channel",
                "ssh.form.single_channel_mode",
                "ssh.form.single_channel_mode_hint",
                ssh_channel_strategy.requires_dedicated_consumers(),
                false,
                |form| {
                    form.ssh_channel_strategy =
                        if form.ssh_channel_strategy.requires_dedicated_consumers() {
                            SshChannelStrategy::Multiplexed
                        } else {
                            // Unsupported shared consumers are disabled at the policy boundary.
                            form.agent_forwarding = false;
                            form.x11_forwarding.enabled = false;
                            SshChannelStrategy::DedicatedPerConsumer
                        };
                },
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_connection_switch_row_with_help(
                "new-connection-dedicated-terminal-help",
                "new-connection-dedicated-terminal",
                "ssh.form.dedicated_new_terminal_connection",
                "ssh.form.dedicated_new_terminal_connection_hint",
                dedicated_new_terminal_connection
                    || ssh_channel_strategy.requires_dedicated_consumers(),
                ssh_channel_strategy.requires_dedicated_consumers(),
                |form| {
                    form.dedicated_new_terminal_connection =
                        !form.dedicated_new_terminal_connection;
                },
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_open_sftp_switch(open_sftp_on_connect, cx))
            .child(self.form_hairline())
            .child(self.render_connection_switch_row_with_help(
                "new-connection-agent-forwarding-help",
                "new-connection-agent-forwarding",
                "ssh.form.agent_forwarding",
                "ssh.form.agent_forwarding_hint",
                agent_forwarding,
                ssh_channel_strategy.requires_dedicated_consumers(),
                |form| form.agent_forwarding = !form.agent_forwarding,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_connection_switch_row_with_help(
                "new-connection-x11-forwarding-help",
                "new-connection-x11-forwarding",
                "ssh.form.x11_forwarding",
                "ssh.form.x11_forwarding_hint",
                x11_forwarding.enabled,
                ssh_channel_strategy.requires_dedicated_consumers(),
                |form| form.x11_forwarding.enabled = !form.x11_forwarding.enabled,
                cx,
            ))
            .when(
                cfg!(target_os = "windows") && x11_forwarding.enabled,
                |content| {
                    content.child(div().pb(px(8.0)).child(
                        self.render_connection_hint(self.i18n.t("ssh.x11.windows_requirement")),
                    ))
                },
            )
            .when(
                x11_forwarding.enabled && !ssh_channel_strategy.requires_dedicated_consumers(),
                |content| {
                    content.child(render_inline_select_row(
                        NewConnectionSelect::X11ForwardingMode,
                        "new-connection-x11-mode-help",
                        "new-connection-x11-mode",
                        "ssh.form.x11_mode",
                        "ssh.form.x11_mode_hint",
                        x11_mode_label,
                        cx,
                    ))
                },
            )
            .when(
                x11_forwarding.enabled
                    && !ssh_channel_strategy.requires_dedicated_consumers()
                    && x11_forwarding.mode == ConnectionX11ForwardingMode::Untrusted,
                |content| {
                    content.child(render_inline_select_row(
                        NewConnectionSelect::X11UntrustedTimeout,
                        "new-connection-x11-timeout-help",
                        "new-connection-x11-timeout",
                        "ssh.form.x11_timeout",
                        "ssh.form.x11_timeout_hint",
                        x11_timeout_label,
                        cx,
                    ))
                },
            )
            .child(self.form_hairline())
            .child(self.render_connection_switch_row(
                "ssh-kerberos",
                self.i18n.t("ssh.form.kerberos_preferred"),
                gssapi_enabled,
                false,
                |form| {
                    form.gssapi_enabled = !form.gssapi_enabled;
                },
                cx,
            ))
            .when(gssapi_enabled, |sec| {
                sec.child(
                    div()
                        .pl(px(14.0))
                        .pb(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(self.render_connection_hint(self.i18n.t("ssh.form.gssapi_desc")))
                        .child(self.render_connection_field(
                            self.i18n.t("ssh.form.gssapi_server_identity"),
                            &gssapi_server_identity,
                            self.i18n.t("ssh.form.gssapi_server_identity_placeholder"),
                            NewConnectionField::GssapiServerIdentity,
                            false,
                            cx,
                        ))
                        .child(
                            self.render_kerberos_credentials_status(gssapi_credentials_available),
                        )
                        .child(self.render_connection_switch_row_with_warning(
                            "kerberos-delegation-help",
                            "kerberos-delegation-tooltip",
                            "ssh.form.gssapi_delegate_credentials",
                            "ssh.form.gssapi_delegation_warning",
                            gssapi_delegate_credentials,
                            false,
                            |form| {
                                form.gssapi_delegate_credentials =
                                    !form.gssapi_delegate_credentials;
                            },
                            cx,
                        )),
                )
            })
            .child(self.form_hairline())
            .child(self.render_connection_switch_row_with_help(
                "new-connection-legacy-ssh-compatibility-help",
                "new-connection-legacy-ssh-compatibility",
                "ssh.form.legacy_ssh_compatibility",
                "ssh.form.legacy_ssh_compatibility_hint",
                legacy_ssh_compatibility,
                false,
                |form| form.legacy_ssh_compatibility = !form.legacy_ssh_compatibility,
                cx,
            ))
            .child(self.form_hairline())
            .child(self.render_connection_disclosure_row(
                "ssh-summary-SshAlgorithms",
                LucideIcon::Lock,
                self.i18n.t("ssh.form.ssh_algorithms"),
                algorithms_summary,
                ssh_algorithms_custom_count > 0,
                crate::form_state::ConnectionFormDrawer::SshAlgorithms,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_standalone_sftp_options(
        &self,
        initial_remote_path: &str,
        connect_timeout_seconds_text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_standalone_sftp_endpoint_options(
            initial_remote_path,
            connect_timeout_seconds_text,
            false,
            cx,
        )
    }

    pub(super) fn render_standalone_sftp_secondary_options(
        &self,
        initial_remote_path: &str,
        connect_timeout_seconds_text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_standalone_sftp_endpoint_options(
            initial_remote_path,
            connect_timeout_seconds_text,
            true,
            cx,
        )
    }

    fn render_standalone_sftp_endpoint_options(
        &self,
        initial_remote_path: &str,
        connect_timeout_seconds_text: &str,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let timeout_row = div()
            .w_full()
            .min_h(px(44.0))
            .py(px(6.0))
            .px(px(2.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(self.render_connection_label_with_help(
                        if secondary {
                            "sftp-secondary-timeout-help"
                        } else {
                            "sftp-primary-timeout-help"
                        },
                        if secondary {
                            "sftp-secondary-timeout"
                        } else {
                            "sftp-primary-timeout"
                        },
                        "ssh.form.connect_timeout",
                        "ssh.form.connect_timeout_hint",
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(84.0))
                            .child(self.render_connection_centered_mono_input(
                                connect_timeout_seconds_text,
                                crate::form_model::DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS.to_string(),
                                if secondary {
                                    NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds
                                } else {
                                    NewConnectionField::ConnectTimeoutSeconds
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(SharedString::from(
                                self.tokens.metrics.markdown_code_font_family,
                            ))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child("s"),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(self.render_connection_field(
                self.i18n.t("sftp.standalone.initial_path"),
                initial_remote_path,
                self.i18n.t("sftp.standalone.initial_path_placeholder"),
                if secondary {
                    NewConnectionField::StandaloneSftpSecondaryInitialRemotePath
                } else {
                    NewConnectionField::InitialRemotePath
                },
                false,
                cx,
            ))
            .child(self.render_connection_hint(self.i18n.t("sftp.standalone.initial_path_hint")))
            .when(self.environment.browser.is_none(), |col| col.child(self.form_hairline()).child(timeout_row))
            .into_any_element()
    }

    pub(super) fn connection_line_ending_label(
        &self,
        value: crate::form_model::SerialLineEnding,
    ) -> String {
        use crate::form_model::SerialLineEnding;
        self.i18n.t(match value {
            SerialLineEnding::None => "terminal.serial_control.line_ending_none",
            SerialLineEnding::Lf => "terminal.serial_control.line_ending_lf",
            SerialLineEnding::CrLf => "terminal.serial_control.line_ending_crlf",
            SerialLineEnding::Cr => "terminal.serial_control.line_ending_cr",
        })
    }

    pub(super) fn connection_send_mode_label(
        &self,
        value: crate::form_model::SerialSendMode,
    ) -> String {
        use crate::form_model::SerialSendMode;
        self.i18n.t(match value {
            SerialSendMode::Text => "terminal.serial_control.text_mode",
            SerialSendMode::Hex => "terminal.serial_control.hex_mode",
        })
    }

    pub(super) fn connection_display_mode_label(
        &self,
        value: crate::form_model::SerialDisplayMode,
    ) -> String {
        use crate::form_model::SerialDisplayMode;
        self.i18n.t(match value {
            SerialDisplayMode::Text => "terminal.serial_control.text_mode",
            SerialDisplayMode::Hex => "terminal.serial_control.hex_mode",
            SerialDisplayMode::Mixed => "terminal.serial_control.mixed_mode",
        })
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
        self.request(FormAction::ClearComposition, None, cx);
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
        self.request(FormAction::ClearComposition, None, cx);
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
        self.request(FormAction::ClearComposition, None, cx);
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
        self.request(FormAction::ClearComposition, None, cx);
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_terminal_session_log_policy(
        &mut self,
        policy: ConnectionTerminalSessionLogPolicy,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.session_log_policy = policy;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_terminal_bell_policy(
        &mut self,
        policy: ConnectionTerminalBellPolicy,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.terminal.bell_policy = policy;
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
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
        selected: crate::form_model::SerialParity,
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
        selected: crate::form_model::SerialFlowControl,
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
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((policy, protocol, host, port, no_proxy, remote_dns, auth, username)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                if secondary {
                    let route = &form.standalone_sftp_secondary;
                    (
                        route.upstream_proxy_policy,
                        route.upstream_proxy_protocol,
                        route.upstream_proxy_host.clone(),
                        route.upstream_proxy_port.clone(),
                        route.upstream_proxy_no_proxy.clone(),
                        route.upstream_proxy_remote_dns,
                        route.upstream_proxy_auth,
                        route.upstream_proxy_username.clone(),
                    )
                } else {
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
                }
            })
        else {
            return div().into_any_element();
        };
        let custom = policy == NewConnectionUpstreamProxyPolicy::Custom;
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x55))
            .pt(px(14.0))
            .child(form_field(
                &self.tokens,
                self.i18n.t("modals.upstream_proxy.policy"),
                self.render_new_connection_select_control(
                    if secondary {
                        NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyPolicy
                    } else {
                        NewConnectionSelect::UpstreamProxyPolicy
                    },
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
                                    if secondary {
                                        NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyProtocol
                                    } else {
                                        NewConnectionSelect::UpstreamProxyProtocol
                                    },
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
                                    if secondary {
                                        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort
                                    } else {
                                        NewConnectionField::UpstreamProxyPort
                                    },
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(self.render_connection_field(
                        self.i18n.t("settings_view.network.host"),
                        &host,
                        "127.0.0.1".to_string(),
                        if secondary {
                            NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost
                        } else {
                            NewConnectionField::UpstreamProxyHost
                        },
                        false,
                        cx,
                    ))
                    .child(self.render_connection_field(
                        self.i18n.t("settings_view.network.no_proxy"),
                        &no_proxy,
                        "localhost,127.0.0.1,*.internal".to_string(),
                        if secondary {
                            NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy
                        } else {
                            NewConnectionField::UpstreamProxyNoProxy
                        },
                        false,
                        cx,
                    ))
                    .child(self.render_connection_switch_row(
                        format!("remote-dns-{secondary}"),
                        self.i18n.t("settings_view.network.remote_dns"),
                        remote_dns,
                        false,
                        move |form| {
                            if secondary {
                                let route = &mut form.standalone_sftp_secondary;
                                route.upstream_proxy_remote_dns =
                                    !route.upstream_proxy_remote_dns;
                            } else {
                                form.upstream_proxy_remote_dns = !form.upstream_proxy_remote_dns;
                            }
                        },
                        cx,
                    ))
                    .child(form_field(
                        &self.tokens,
                        self.i18n.t("settings_view.network.auth"),
                        self.render_new_connection_select_control(
                            if secondary {
                                NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyAuth
                            } else {
                                NewConnectionSelect::UpstreamProxyAuth
                            },
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
                                    if secondary {
                                        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername
                                    } else {
                                        NewConnectionField::UpstreamProxyUsername
                                    },
                                    false,
                                    cx,
                                ))
                                .child(self.render_connection_secret_field(
                                    self.i18n.t("settings_view.network.password"),
                                    String::new(),
                                    if secondary {
                                        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword
                                    } else {
                                        NewConnectionField::UpstreamProxyPassword
                                    },
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

    pub(super) fn serial_parity_label(&self, parity: crate::form_model::SerialParity) -> String {
        match parity {
            crate::form_model::SerialParity::None => {
                self.i18n.t("modals.new_connection.serial_parity_none")
            }
            crate::form_model::SerialParity::Odd => {
                self.i18n.t("modals.new_connection.serial_parity_odd")
            }
            crate::form_model::SerialParity::Even => {
                self.i18n.t("modals.new_connection.serial_parity_even")
            }
        }
    }

    pub(super) fn serial_flow_control_label(
        &self,
        flow: crate::form_model::SerialFlowControl,
    ) -> String {
        match flow {
            crate::form_model::SerialFlowControl::None => {
                self.i18n.t("modals.new_connection.serial_flow_none")
            }
            crate::form_model::SerialFlowControl::Software => {
                self.i18n.t("modals.new_connection.serial_flow_software")
            }
            crate::form_model::SerialFlowControl::Hardware => {
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
        self.request(FormAction::ClearComposition, None, cx);
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_parity(
        &mut self,
        parity: crate::form_model::SerialParity,
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_flow_control(
        &mut self,
        flow: crate::form_model::SerialFlowControl,
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
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_policy(
        &mut self,
        policy: NewConnectionUpstreamProxyPolicy,
        secondary: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                if secondary {
                    form.standalone_sftp_secondary.upstream_proxy_policy = policy;
                } else {
                    form.upstream_proxy_policy = policy;
                }
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_protocol(
        &mut self,
        protocol: SavedUpstreamProxyProtocol,
        secondary: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                if secondary {
                    form.standalone_sftp_secondary.upstream_proxy_protocol = protocol;
                } else {
                    form.upstream_proxy_protocol = protocol;
                }
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_auth(
        &mut self,
        auth: NewConnectionUpstreamProxyAuth,
        secondary: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                if secondary {
                    let route = &mut form.standalone_sftp_secondary;
                    if auth == NewConnectionUpstreamProxyAuth::None {
                        // Hidden password drafts remain owned by this endpoint and are scrubbed.
                        zeroize::Zeroize::zeroize(&mut route.upstream_proxy_password);
                        route.upstream_proxy_password_keychain_id = None;
                    }
                    route.upstream_proxy_auth = auth;
                } else {
                    if auth == NewConnectionUpstreamProxyAuth::None {
                        // Hidden password fields should not retain a draft secret after
                        // switching the custom proxy back to unauthenticated mode.
                        zeroize::Zeroize::zeroize(&mut form.upstream_proxy_password);
                        form.upstream_proxy_password_keychain_id = None;
                    }
                    form.upstream_proxy_auth = auth;
                }
                form.field_focused = false;
                clear_connection_selection(form);
                form.error = None;
            }
        });
        self.request(FormAction::ClearComposition, None, cx);
        cx.notify();
    }

    pub(super) fn form_hairline(&self) -> Div {
        div()
            .w_full()
            .h(px(1.0))
            .bg(rgba((self.tokens.ui.border << 8) | 0x55))
    }

    /// Flat hairline-list disclosure row that pushes into a dedicated form subpage
    /// while summarizing its current non-default state inline.
    pub(super) fn render_connection_disclosure_row(
        &self,
        debug_id: &'static str,
        icon: LucideIcon,
        title: String,
        summary: String,
        highlighted: bool,
        target: crate::form_state::ConnectionFormDrawer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .id(SharedString::from(debug_id))
            .debug_selector(move || debug_id.to_string())
            .w_full()
            .min_h(px(36.0))
            .px(px(6.0))
            .py(px(5.0))
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x80)))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(9.0))
                    .child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(if highlighted {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(title),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap(px(6.0))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(if highlighted {
                                theme.text
                            } else {
                                theme.text_muted
                            }))
                            .child(summary),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        14.0,
                        rgb(theme.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.close_new_connection_select(cx);
                    this.reset_studio_form_scroll();
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.active_drawer = Some(target);
                            form.ssh_algorithm_menu = None;
                            form.field_focused = false;
                            clear_connection_selection(form);
                        }
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_connection_switch_row(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        checked: bool,
        disabled: bool,
        toggle: impl Fn(&mut NewConnectionForm) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_connection_switch_row_content(id, label, None, checked, disabled, toggle, cx)
    }

    pub(super) fn render_connection_switch_row_with_help(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        disabled: bool,
        toggle: impl Fn(&mut NewConnectionForm) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let help = self.render_connection_help_icon(trigger_id, tooltip_id, hint_key, cx);
        self.render_connection_switch_row_content(
            format!("switch-{trigger_id}"),
            self.i18n.t(label_key),
            Some(help),
            checked,
            disabled,
            toggle,
            cx,
        )
    }

    pub(super) fn render_connection_switch_row_with_warning(
        &self,
        trigger_id: &'static str,
        tooltip_id: &'static str,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        disabled: bool,
        toggle: impl Fn(&mut NewConnectionForm) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let warning = self.render_connection_help_icon_with_icon(
            trigger_id,
            tooltip_id,
            hint_key,
            LucideIcon::AlertCircle,
            cx,
        );
        self.render_connection_switch_row_content(
            format!("switch-{trigger_id}"),
            self.i18n.t(label_key),
            Some(warning),
            checked,
            disabled,
            toggle,
            cx,
        )
    }

    fn render_connection_switch_row_content(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        help_icon: Option<AnyElement>,
        checked: bool,
        disabled: bool,
        toggle: impl Fn(&mut NewConnectionForm) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let row_id = id.into();
        let switch_id = SharedString::from(format!("track-{row_id:?}"));
        let row = div()
            .id(row_id)
            .w_full()
            .min_h(px(34.0))
            .py(px(4.0))
            .px(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .when(!disabled, |row| {
                row.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x45)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(if disabled {
                                theme.text_muted
                            } else {
                                theme.text
                            }))
                            .child(label),
                    )
                    .when_some(help_icon, |row, icon| row.child(icon)),
            )
            .child(switch_with(
                &self.tokens,
                switch_id,
                "",
                checked,
                SwitchOptions::default().disabled(disabled),
            ));

        if disabled {
            row.into_any_element()
        } else {
            row.on_mouse_down(
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
    }

    pub(super) fn render_connection_checkbox(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        checked: bool,
        toggle: impl Fn(&mut NewConnectionForm) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, id, label, checked)
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
                    let standalone_sftp =
                        this.connection_form_state(cx)
                            .form
                            .as_ref()
                            .is_some_and(|form| {
                                form.transport == NewConnectionTransport::StandaloneSftp
                            });
                    this.start_new_connection_flow(standalone_sftp, window, cx);
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

    /// Split primary action button (`Save & Connect | ▾`) that keeps one-off
    /// `Connect` reachable without crowding the footer with four equal buttons.
    pub(super) fn render_split_primary_action_button(
        &self,
        primary_label: String,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let height = self.tokens.metrics.ui_control_height;
        let bg = if disabled {
            rgba((theme.accent << 8) | 0x66)
        } else {
            rgb(theme.accent)
        };
        let fg = rgb(theme.accent_text);
        let divider_color = rgba((theme.accent_text << 8) | 0x38);
        let hover_overlay = rgba((theme.accent_text << 8) | 0x16);

        let group = div()
            .h(px(height))
            .rounded(px(self.tokens.radii.md))
            .bg(bg)
            .overflow_hidden()
            .flex()
            .flex_row()
            .items_center()
            .when(disabled, |el| el.opacity(0.65))
            .child(
                div()
                    .id("connection-primary-save-and-connect")
                    .h_full()
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(fg)
                    .whitespace_nowrap()
                    .when(!disabled, |el| {
                        el.cursor_pointer().hover(|h| h.bg(hover_overlay))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if disabled {
                                return;
                            }
                            this.close_new_connection_select(cx);
                            this.submit_new_connection_form_with_action(
                                NewConnectionSubmitAction::SaveAndConnect,
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .child(primary_label),
            )
            .child(div().w(px(1.0)).h(px(16.0)).bg(divider_color))
            .child(
                div()
                    .id("connection-primary-action-menu")
                    .debug_selector(|| "connection-primary-action-menu".to_string())
                    .h_full()
                    .w(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!disabled, |el| {
                        el.cursor_pointer().hover(|h| h.bg(hover_overlay))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if disabled {
                                return;
                            }
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.field_focused = false;
                                    form.selected_field = None;
                                }
                            });
                            this.request(FormAction::ClearComposition, None, cx);
                            this.open_new_connection_select_from_pointer(
                                NewConnectionSelect::PrimaryActionMenu,
                                cx,
                            );
                            this.request(FormAction::FocusForm, Some(window), cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(LucideIcon::ChevronDown, 13.0, fg)),
            );

        self.track_new_connection_select_anchor(NewConnectionSelect::PrimaryActionMenu, group, cx)
    }
}

pub(super) fn serial_port_display_label(port: &crate::form_model::SerialPortInfo) -> String {
    if port.display_name.trim().is_empty() {
        port.port_path.clone()
    } else {
        port.display_name.clone()
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

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
