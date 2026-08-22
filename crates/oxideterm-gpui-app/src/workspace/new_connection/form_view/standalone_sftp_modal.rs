use super::*;
use gpui::{App, Point, ScrollHandle, ScrollWheelEvent};
use oxideterm_gpui_ui::ScrollableElement;

const STANDALONE_SFTP_COMMON_COLUMN_WIDTH: f32 = 320.0;
const STANDALONE_SFTP_ENDPOINT_COLUMN_WIDTH: f32 = 390.0;
const STANDALONE_SFTP_VIEWPORT_MARGIN: f32 = 32.0;

fn handle_standalone_sftp_column_scroll(
    scroll_handle: &ScrollHandle,
    event: &ScrollWheelEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let delta = event.delta.pixel_delta(window.line_height());
    if delta.x.abs() > delta.y.abs() {
        return;
    }

    let old_offset = scroll_handle.offset();
    let max_offset = scroll_handle.max_offset();
    let next_y = (old_offset.y + delta.y).clamp(-max_offset.y, px(0.0));
    if next_y != old_offset.y {
        scroll_handle.set_offset(Point::new(old_offset.x, next_y));
        window.refresh();
    }

    // Consume vertical gestures even at a column boundary so the horizontal
    // viewport never treats the remaining wheel movement as horizontal input.
    cx.stop_propagation();
}

struct StandaloneSftpModalSnapshot {
    profile_id: Option<String>,
    transfer_mode: StandaloneSftpTransferMode,
    name: String,
    group: String,
    notes: String,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
    key_path: String,
    managed_key_id: String,
    cert_path: String,
    identity_agent: String,
    gssapi_enabled: bool,
    gssapi_server_identity: String,
    gssapi_delegate_credentials: bool,
    agent_available: Option<bool>,
    saved_credential_present: bool,
    save_password: bool,
    initial_remote_path: String,
    connect_timeout_seconds_text: String,
    proxy_command_enabled: bool,
    proxy_command: zeroize::Zeroizing<String>,
    proxy_command_configured: bool,
    secondary_host: String,
    secondary_port: String,
    secondary_username: String,
    secondary_auth_tab: SshAuthTab,
    secondary_key_path: String,
    secondary_managed_key_id: String,
    secondary_cert_path: String,
    secondary_identity_agent: String,
    secondary_gssapi_enabled: bool,
    secondary_gssapi_server_identity: String,
    secondary_gssapi_delegate_credentials: bool,
    secondary_agent_available: Option<bool>,
    secondary_saved_credential_present: bool,
    secondary_save_password: bool,
    secondary_initial_remote_path: String,
    secondary_connect_timeout_seconds_text: String,
    secondary_proxy_command_enabled: bool,
    secondary_proxy_command: zeroize::Zeroizing<String>,
    secondary_proxy_command_configured: bool,
    pending: bool,
    error: Option<String>,
    feedback_success: bool,
    gssapi_credentials_available: Option<bool>,
}

impl StandaloneSftpModalSnapshot {
    fn from_form(form: &NewConnectionForm) -> Self {
        Self {
            profile_id: form.standalone_sftp_profile_id.clone(),
            transfer_mode: form.standalone_sftp_transfer_mode,
            name: form.name.clone(),
            group: form.group.clone(),
            notes: form.notes.clone(),
            host: form.host.clone(),
            port: form.port.clone(),
            username: form.username.clone(),
            auth_tab: form.auth_tab,
            key_path: form.key_path.clone(),
            managed_key_id: form.managed_key_id.clone(),
            cert_path: form.cert_path.clone(),
            identity_agent: form.identity_agent.clone(),
            gssapi_enabled: form.gssapi_enabled,
            gssapi_server_identity: form.gssapi_server_identity.clone(),
            gssapi_delegate_credentials: form.gssapi_delegate_credentials,
            agent_available: form.agent_available,
            saved_credential_present: form.saved_password_keychain_id.is_some(),
            save_password: form.save_password,
            initial_remote_path: form.sftp_initial_remote_path.clone(),
            connect_timeout_seconds_text: form.connect_timeout_seconds_text.clone(),
            proxy_command_enabled: form.proxy_command_enabled,
            // Rendering owns one bounded zeroizing copy of the protected command draft.
            proxy_command: zeroize::Zeroizing::new(form.proxy_command.clone()),
            proxy_command_configured: form.proxy_command_keychain_id.is_some(),
            secondary_host: form.standalone_sftp_secondary.host.clone(),
            secondary_port: form.standalone_sftp_secondary.port.clone(),
            secondary_username: form.standalone_sftp_secondary.username.clone(),
            secondary_auth_tab: form.standalone_sftp_secondary.auth_tab,
            secondary_key_path: form.standalone_sftp_secondary.key_path.clone(),
            secondary_managed_key_id: form.standalone_sftp_secondary.managed_key_id.clone(),
            secondary_cert_path: form.standalone_sftp_secondary.cert_path.clone(),
            secondary_identity_agent: form.standalone_sftp_secondary.identity_agent.clone(),
            secondary_gssapi_enabled: form.standalone_sftp_secondary.gssapi_enabled,
            secondary_gssapi_server_identity: form
                .standalone_sftp_secondary
                .gssapi_server_identity
                .clone(),
            secondary_gssapi_delegate_credentials: form
                .standalone_sftp_secondary
                .gssapi_delegate_credentials,
            secondary_agent_available: form.standalone_sftp_secondary.agent_available,
            secondary_saved_credential_present: form
                .standalone_sftp_secondary
                .password_keychain_id
                .is_some(),
            secondary_save_password: form.standalone_sftp_secondary.save_password,
            secondary_initial_remote_path: form
                .standalone_sftp_secondary
                .initial_remote_path
                .clone(),
            secondary_connect_timeout_seconds_text: form
                .standalone_sftp_secondary
                .connect_timeout_seconds_text
                .clone(),
            secondary_proxy_command_enabled: form.standalone_sftp_secondary.proxy_command_enabled,
            // Each endpoint keeps its protected command draft in a separate zeroizing owner.
            secondary_proxy_command: zeroize::Zeroizing::new(
                form.standalone_sftp_secondary.proxy_command.clone(),
            ),
            secondary_proxy_command_configured: form
                .standalone_sftp_secondary
                .proxy_command_keychain_id
                .is_some(),
            pending: form.pending,
            error: form.error.clone(),
            feedback_success: form.feedback_is_success(),
            gssapi_credentials_available: form.gssapi_credentials_available,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn render_standalone_sftp_connection_modal(
        &self,
        window: &Window,
        shows_transport_selector: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(StandaloneSftpModalSnapshot::from_form)
        else {
            return div().into_any_element();
        };
        if (form.gssapi_enabled || form.secondary_gssapi_enabled)
            && form.gssapi_credentials_available.is_none()
        {
            self.ensure_kerberos_credentials_availability(cx);
        }
        let remote_to_remote = form.transfer_mode == StandaloneSftpTransferMode::RemoteRemote;
        let endpoint_count = if remote_to_remote { 2.0 } else { 1.0 };
        let sidebar_width = if shows_transport_selector {
            NEW_CONNECTION_TYPE_SIDEBAR_WIDTH + self.tokens.metrics.modal_section_gap
        } else {
            0.0
        };
        let desired_width = sidebar_width
            + STANDALONE_SFTP_COMMON_COLUMN_WIDTH
            + endpoint_count
                * (STANDALONE_SFTP_ENDPOINT_COLUMN_WIDTH + self.tokens.metrics.modal_section_gap);
        let modal_width = desired_width
            .min(f32::from(window.viewport_size().width) - STANDALONE_SFTP_VIEWPORT_MARGIN);
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let form_visible = self.connection_form_state(cx).presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let primary_complete = !form.host.trim().is_empty()
            && !form.username.trim().is_empty()
            && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
            && form
                .connect_timeout_seconds_text
                .trim()
                .parse::<u64>()
                .is_ok_and(|seconds| seconds > 0);
        let secondary_complete = !remote_to_remote
            || (!form.secondary_host.trim().is_empty()
                && !form.secondary_username.trim().is_empty()
                && form
                    .secondary_port
                    .trim()
                    .parse::<u16>()
                    .is_ok_and(|port| port > 0)
                && form
                    .secondary_connect_timeout_seconds_text
                    .trim()
                    .parse::<u64>()
                    .is_ok_and(|seconds| seconds > 0));
        let primary_disabled = form.pending || !primary_complete || !secondary_complete;
        let title = if form.profile_id.is_some() {
            self.i18n.t("sessionManager.edit_properties.title")
        } else {
            self.i18n.t("sftp.standalone.form_title")
        };

        modal_backdrop(dialog_backdrop_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.close_new_connection_form(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "standalone-sftp-form-enter",
                modal_container(&self.tokens)
                    .w(px(modal_width))
                    .max_h(px(modal_max_height))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(modal_header(
                        &self.tokens,
                        title,
                        self.i18n.t("sftp.standalone.form_description"),
                    ))
                    .child(
                        modal_body(&self.tokens)
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_x_scrollbar()
                            .child(
                                div()
                                    .h_full()
                                    .min_w(px(desired_width))
                                    .flex()
                                    .gap(px(self.tokens.metrics.modal_section_gap))
                                    .when(shows_transport_selector, |content| {
                                        content.child(self.render_transport_selector(cx))
                                    })
                                    .child(self.render_standalone_sftp_common_column(&form, cx))
                                    .child(self.render_standalone_sftp_endpoint_column(
                                        &form,
                                        false,
                                        remote_to_remote,
                                        cx,
                                    ))
                                    .when(remote_to_remote, |content| {
                                        content.child(self.render_standalone_sftp_endpoint_column(
                                            &form, true, true, cx,
                                        ))
                                    }),
                            ),
                    )
                    .child(
                        modal_footer(&self.tokens)
                            .flex_none()
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.cancel"),
                                false,
                                ConnectionButtonAction::Cancel,
                                false,
                                cx,
                            ))
                            .when(form.profile_id.is_none(), |footer| {
                                footer
                                    .when(!remote_to_remote, |footer| {
                                        footer.child(self.render_connection_button(
                                            self.i18n.t("ssh.form.test"),
                                            false,
                                            ConnectionButtonAction::Test,
                                            primary_disabled,
                                            cx,
                                        ))
                                    })
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save"),
                                        false,
                                        ConnectionButtonAction::Save,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .when(!remote_to_remote, |footer| {
                                        footer.child(self.render_connection_button(
                                            self.i18n.t("sftp.standalone.open"),
                                            false,
                                            ConnectionButtonAction::Connect,
                                            primary_disabled,
                                            cx,
                                        ))
                                    })
                                    .child(self.render_connection_button(
                                        self.i18n.t("sftp.standalone.save_and_open"),
                                        true,
                                        ConnectionButtonAction::SaveAndConnect,
                                        primary_disabled,
                                        cx,
                                    ))
                            })
                            .when(form.profile_id.is_some(), |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("sessionManager.edit_properties.save"),
                                    true,
                                    ConnectionButtonAction::Save,
                                    primary_disabled,
                                    cx,
                                ))
                            }),
                    ),
                form_visible,
            ))
            .into_any_element()
    }

    fn render_standalone_sftp_common_column(
        &self,
        form: &StandaloneSftpModalSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scroll_handle = self.selectable_text_scroll_handle("standalone-sftp-common-column");
        let wheel_scroll_handle = scroll_handle.clone();
        div()
            .id("standalone-sftp-common-column")
            .w(px(STANDALONE_SFTP_COMMON_COLUMN_WIDTH))
            .min_w(px(STANDALONE_SFTP_COMMON_COLUMN_WIDTH))
            .h_full()
            .border_r_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
            .overflow_hidden()
            .track_scroll(&scroll_handle)
            .on_scroll_wheel(move |event, window, cx| {
                handle_standalone_sftp_column_scroll(&wheel_scroll_handle, event, window, cx);
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.metrics.modal_section_gap))
                    .pr(px(self.tokens.metrics.modal_section_gap))
                    .child(
                        div()
                            .p_3()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(self.tokens.ui.warning))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("sftp.standalone.usage_notice")),
                    )
                    .child(self.render_standalone_sftp_transfer_mode(form.transfer_mode, cx))
                    .child(self.render_connection_field(
                        self.i18n.t("ssh.form.name"),
                        &form.name,
                        self.i18n.t("ssh.form.name_placeholder"),
                        NewConnectionField::Name,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_group_select(
                        self.i18n.t("ssh.form.group"),
                        &form.group,
                        cx,
                    ))
                    .child(self.render_connection_notes_fields(&form.notes, cx))
                    .when_some(form.error.clone(), |content, error| {
                        content.child(self.render_prompt_feedback_box(error, form.feedback_success))
                    }),
            )
            .into_any_element()
    }

    fn render_standalone_sftp_transfer_mode(
        &self,
        selected: StandaloneSftpTransferMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let options = [
            (
                StandaloneSftpTransferMode::LocalRemote,
                "sftp.standalone.mode_local_remote",
            ),
            (
                StandaloneSftpTransferMode::RemoteRemote,
                "sftp.standalone.mode_remote_remote",
            ),
        ];
        let tabs = options
            .into_iter()
            .map(|(mode, label)| {
                segmented_tab(&self.tokens, self.i18n.t(label), mode == selected)
                    .id(SharedString::from(format!("standalone-sftp-mode-{mode:?}")))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.set_standalone_sftp_transfer_mode(mode);
                                    form.error = None;
                                }
                            });
                            this.close_new_connection_select(cx);
                            cx.notify();
                        }),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(form_field(
                &self.tokens,
                self.i18n.t("sftp.standalone.transfer_mode"),
                segmented_tabs(&self.tokens).children(tabs),
            ))
            .child(self.render_connection_hint(self.i18n.t(match selected {
                StandaloneSftpTransferMode::LocalRemote => "sftp.standalone.mode_local_remote_hint",
                StandaloneSftpTransferMode::RemoteRemote => {
                    "sftp.standalone.mode_remote_remote_hint"
                }
            })))
            .into_any_element()
    }

    fn render_standalone_sftp_endpoint_column(
        &self,
        form: &StandaloneSftpModalSnapshot,
        secondary: bool,
        numbered: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (
            host,
            port,
            username,
            auth_tab,
            key_path,
            managed_key_id,
            cert_path,
            identity_agent,
            gssapi_server_identity,
            gssapi_delegate_credentials,
            agent_available,
            saved_credential_present,
            save_password,
            initial_remote_path,
            connect_timeout_seconds_text,
        ) = if secondary {
            (
                form.secondary_host.as_str(),
                form.secondary_port.as_str(),
                form.secondary_username.as_str(),
                form.secondary_auth_tab,
                form.secondary_key_path.as_str(),
                form.secondary_managed_key_id.as_str(),
                form.secondary_cert_path.as_str(),
                form.secondary_identity_agent.as_str(),
                form.secondary_gssapi_server_identity.as_str(),
                form.secondary_gssapi_delegate_credentials,
                form.secondary_agent_available,
                form.secondary_saved_credential_present,
                form.secondary_save_password,
                form.secondary_initial_remote_path.as_str(),
                form.secondary_connect_timeout_seconds_text.as_str(),
            )
        } else {
            (
                form.host.as_str(),
                form.port.as_str(),
                form.username.as_str(),
                form.auth_tab,
                form.key_path.as_str(),
                form.managed_key_id.as_str(),
                form.cert_path.as_str(),
                form.identity_agent.as_str(),
                form.gssapi_server_identity.as_str(),
                form.gssapi_delegate_credentials,
                form.agent_available,
                form.saved_credential_present,
                form.save_password,
                form.initial_remote_path.as_str(),
                form.connect_timeout_seconds_text.as_str(),
            )
        };
        let host_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryHost
        } else {
            NewConnectionField::Host
        };
        let port_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryPort
        } else {
            NewConnectionField::Port
        };
        let username_field = if secondary {
            NewConnectionField::StandaloneSftpSecondaryUsername
        } else {
            NewConnectionField::Username
        };
        let column_id = if secondary {
            "standalone-sftp-secondary-column"
        } else {
            "standalone-sftp-primary-column"
        };
        let title_key = if numbered {
            if secondary {
                "sftp.standalone.remote_host_2"
            } else {
                "sftp.standalone.remote_host_1"
            }
        } else {
            "sftp.standalone.remote_host"
        };
        let scroll_handle = self.selectable_text_scroll_handle(column_id);
        let wheel_scroll_handle = scroll_handle.clone();
        let options = if secondary {
            self.render_standalone_sftp_secondary_options(
                initial_remote_path,
                connect_timeout_seconds_text,
                cx,
            )
        } else {
            self.render_standalone_sftp_options(
                initial_remote_path,
                connect_timeout_seconds_text,
                cx,
            )
        };

        div()
            .id(column_id)
            .w(px(STANDALONE_SFTP_ENDPOINT_COLUMN_WIDTH))
            .min_w(px(STANDALONE_SFTP_ENDPOINT_COLUMN_WIDTH))
            .h_full()
            .when(numbered && !secondary, |column| {
                column
                    .border_r_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
            })
            .overflow_hidden()
            .track_scroll(&scroll_handle)
            .on_scroll_wheel(move |event, window, cx| {
                handle_standalone_sftp_column_scroll(&wheel_scroll_handle, event, window, cx);
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.metrics.modal_section_gap))
                    .pr(px(self.tokens.metrics.modal_section_gap))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(self.tokens.metrics.form_host_port_gap))
                            .child(div().flex_1().child(self.render_connection_field(
                                self.i18n.t("sftp.standalone.host"),
                                host,
                                self.i18n.t("sftp.standalone.host_placeholder"),
                                host_field,
                                false,
                                cx,
                            )))
                            .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                                self.render_connection_field(
                                    self.i18n.t("sftp.standalone.port"),
                                    port,
                                    SSH_DEFAULT_PORT_TEXT.to_string(),
                                    port_field,
                                    false,
                                    cx,
                                ),
                            )),
                    )
                    .child(self.render_connection_field(
                        self.i18n.t("ssh.form.username"),
                        username,
                        "root".to_string(),
                        username_field,
                        false,
                        cx,
                    ))
                    .child(self.render_standalone_sftp_endpoint_auth(
                        auth_tab,
                        secondary,
                        key_path,
                        managed_key_id,
                        cert_path,
                        identity_agent,
                        if secondary {
                            form.secondary_gssapi_enabled
                        } else {
                            form.gssapi_enabled
                        },
                        gssapi_server_identity,
                        gssapi_delegate_credentials,
                        form.gssapi_credentials_available,
                        agent_available,
                        saved_credential_present,
                        save_password,
                        cx,
                    ))
                    .child(options)
                    .child({
                        let (proxy_command_enabled, proxy_command, proxy_command_configured) =
                            if secondary {
                                (
                                    form.secondary_proxy_command_enabled,
                                    form.secondary_proxy_command.as_str(),
                                    form.secondary_proxy_command_configured,
                                )
                            } else {
                                (
                                    form.proxy_command_enabled,
                                    form.proxy_command.as_str(),
                                    form.proxy_command_configured,
                                )
                            };
                        let route = div()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.modal_section_gap))
                            .child(self.render_proxy_command_section(
                                proxy_command_enabled,
                                proxy_command,
                                proxy_command_configured,
                                secondary,
                                cx,
                            ))
                            .when(!proxy_command_enabled, |route| {
                                route
                                    .child(self.render_upstream_proxy_policy_section(secondary, cx))
                                    .child(self.render_proxy_chain_section(secondary, cx))
                            })
                            .into_any_element();
                        self.render_connection_form_section(
                            if secondary {
                                ConnectionFormSection::StandaloneSftpSecondaryRoute
                            } else {
                                ConnectionFormSection::Route
                            },
                            route,
                            cx,
                        )
                    }),
            )
            .into_any_element()
    }
}
