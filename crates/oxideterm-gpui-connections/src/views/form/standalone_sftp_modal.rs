use super::*;
use crate::form_state::ConnectionFormDrawer;
use oxideterm_gpui_ui::scroll::{Scrollbar, ScrollbarAxis};

struct StandaloneSftpModalSnapshot {
    profile_id: Option<String>,
    transfer_mode: StandaloneSftpTransferMode,
    active_drawer: Option<ConnectionFormDrawer>,
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
    empty_password: bool,
    initial_remote_path: String,
    connect_timeout_seconds_text: String,
    proxy_command_enabled: bool,
    proxy_command: zeroize::Zeroizing<String>,
    proxy_command_configured: bool,
    proxy_hops_len: usize,
    upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    route_customized: bool,
    appearance_customized: bool,
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
    secondary_empty_password: bool,
    secondary_initial_remote_path: String,
    secondary_connect_timeout_seconds_text: String,
    secondary_proxy_command_enabled: bool,
    secondary_proxy_command: zeroize::Zeroizing<String>,
    secondary_proxy_command_configured: bool,
    secondary_proxy_hops_len: usize,
    secondary_upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    pending: bool,
    error: Option<String>,
    feedback_success: bool,
    gssapi_credentials_available: Option<bool>,
}

impl StandaloneSftpModalSnapshot {
    fn from_form(form: &NewConnectionForm) -> Self {
        let remote_to_remote =
            form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote;
        let primary_route_customized = form.proxy_command_enabled
            || !form.proxy_hops.is_empty()
            || form.upstream_proxy_policy != NewConnectionUpstreamProxyPolicy::UseGlobal;
        let secondary_route_customized = remote_to_remote
            && (form.standalone_sftp_secondary.proxy_command_enabled
                || !form.standalone_sftp_secondary.proxy_hops.is_empty()
                || form.standalone_sftp_secondary.upstream_proxy_policy
                    != NewConnectionUpstreamProxyPolicy::UseGlobal);
        let appearance_customized = !form.icon.trim().is_empty()
            || !form.color.trim().is_empty()
            || !form.icon_background_color.trim().is_empty();

        Self {
            profile_id: form.standalone_sftp_profile_id.clone(),
            transfer_mode: form.standalone_sftp_transfer_mode,
            active_drawer: form.active_drawer,
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
            empty_password: form.empty_password,
            initial_remote_path: form.sftp_initial_remote_path.clone(),
            connect_timeout_seconds_text: form.connect_timeout_seconds_text.clone(),
            proxy_command_enabled: form.proxy_command_enabled,
            // Rendering owns one bounded zeroizing copy of the protected command draft.
            proxy_command: zeroize::Zeroizing::new(form.proxy_command.as_str().to_owned()),
            proxy_command_configured: form.proxy_command_keychain_id.is_some(),
            proxy_hops_len: form.proxy_hops.len(),
            upstream_proxy_policy: form.upstream_proxy_policy,
            route_customized: primary_route_customized || secondary_route_customized,
            appearance_customized,
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
            secondary_empty_password: form.standalone_sftp_secondary.empty_password,
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
                form.standalone_sftp_secondary
                    .proxy_command
                    .as_str()
                    .to_owned(),
            ),
            secondary_proxy_command_configured: form
                .standalone_sftp_secondary
                .proxy_command_keychain_id
                .is_some(),
            secondary_proxy_hops_len: form.standalone_sftp_secondary.proxy_hops.len(),
            secondary_upstream_proxy_policy: form.standalone_sftp_secondary.upstream_proxy_policy,
            pending: form.pending,
            error: form.error.clone(),
            feedback_success: form.feedback_is_success(),
            gssapi_credentials_available: form.gssapi_credentials_available,
        }
    }
}

impl ConnectionFormView {
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
        let theme = self.tokens.ui;
        let remote_to_remote = form.transfer_mode == StandaloneSftpTransferMode::RemoteRemote;
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let (modal_width, studio_height) = composer::studio_modal_size(
            window,
            &self.tokens,
            shows_transport_selector,
            modal_max_height,
        );
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

        let active_drawer = if !remote_to_remote
            && form.active_drawer == Some(ConnectionFormDrawer::StandaloneSftpSecondary)
        {
            None
        } else {
            form.active_drawer
        };

        let studio_layout = composer::StudioLayoutMetrics::compute(
            modal_width,
            &self.tokens,
            shows_transport_selector,
        );
        let subpage_header = active_drawer.map(|drawer| {
            let root_label = self
                .i18n
                .t("modals.new_connection.transport_standalone_sftp");
            let current_label = match drawer {
                ConnectionFormDrawer::StandaloneSftpPrimary => self.i18n.t(if remote_to_remote {
                    "sftp.standalone.remote_host_1"
                } else {
                    "sftp.standalone.remote_host"
                }),
                ConnectionFormDrawer::StandaloneSftpSecondary => {
                    self.i18n.t("sftp.standalone.remote_host_2")
                }
                ConnectionFormDrawer::Route => self.i18n.t("ssh.form.connection_route"),
                ConnectionFormDrawer::Notes => self.i18n.t("ssh.form.appearance"),
                _ => root_label.clone(),
            };
            div()
                .flex_none()
                .w_full()
                .h(px(40.0))
                .px(px(studio_layout.content_padding_x - 6.0))
                .bg(rgb(theme.bg_elevated))
                .border_b_1()
                .border_color(rgba((theme.border << 8) | 0xb3))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id("standalone-sftp-subpage-back")
                        .debug_selector(|| "standalone-sftp-subpage-back".to_string())
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(self.tokens.radii.sm))
                        .cursor_pointer()
                        .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::ChevronLeft,
                            14.0,
                            rgb(theme.text_muted),
                        ))
                        .child(root_label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_new_connection_select(cx);
                                this.reset_studio_form_scroll();
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.active_drawer = None;
                                        form.ssh_algorithm_menu = None;
                                        form.field_focused = false;
                                        clear_connection_selection(form);
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgba((theme.text_muted << 8) | 0x88))
                        .child("/"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_heading))
                        .child(current_label),
                )
                .into_any_element()
        });
        let page_content = match active_drawer {
            Some(ConnectionFormDrawer::StandaloneSftpPrimary) => self
                .render_standalone_sftp_endpoint_page(
                    &form,
                    false,
                    remote_to_remote,
                    studio_layout,
                    cx,
                ),
            Some(ConnectionFormDrawer::StandaloneSftpSecondary) if remote_to_remote => {
                self.render_standalone_sftp_endpoint_page(&form, true, true, studio_layout, cx)
            }
            Some(ConnectionFormDrawer::Route) => {
                self.render_standalone_sftp_route_page(&form, remote_to_remote, cx)
            }
            Some(ConnectionFormDrawer::Notes) => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_appearance_options(cx))
                .child(self.form_hairline())
                .child(self.render_connection_notes_fields(&form.notes, cx))
                .into_any_element(),
            _ => self.render_standalone_sftp_basic_page(&form, studio_layout, cx),
        };

        let fields_scroll = self.selectable_text_scroll_handle("standalone-sftp-form-scroll");

        oxideterm_gpui_ui::modal::staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.close_new_connection_form(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
                &self.tokens,
                "new-connection-form-scrim",
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
                true,
            ))
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "new-connection-form-enter",
                oxideterm_gpui_ui::theme_overlay_surface_shadow(
                    modal_container(&self.tokens),
                    &self.tokens,
                )
                .bg(rgb(theme.bg_elevated))
                .overflow_hidden()
                .w(px(modal_width))
                .h(px(studio_height))
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(self.render_studio_top_chrome(
                    title,
                    Some(self.i18n.t("sftp.standalone.form_description")),
                    shows_transport_selector,
                    false,
                    cx,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .flex()
                        .when(shows_transport_selector, |content| {
                            content.child(self.render_studio_protocol_sidebar(studio_layout, cx))
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .min_h(px(0.0))
                                .flex()
                                .flex_col()
                                .when_some(subpage_header, |pane, header| pane.child(header))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .min_h(px(0.0))
                                        .relative()
                                        .child(
                                            div()
                                                .id("standalone-sftp-form-scroll")
                                                .size_full()
                                                .px(px(studio_layout.content_padding_x))
                                                .py(px(16.0))
                                                .overflow_y_scroll()
                                                .track_scroll(&fields_scroll)
                                                .on_scroll(cx.listener(|this, _, _, cx| {
                                                    if this
                                                        .connection_form_state(cx)
                                                        .open_select
                                                        .is_some()
                                                    {
                                                        this.close_new_connection_select(cx);
                                                        this.clear_new_connection_select_anchor(cx);
                                                        cx.notify();
                                                    }
                                                }))
                                                .child(page_content),
                                        )
                                        .child(
                                            Scrollbar::new(&fields_scroll)
                                                .axis(ScrollbarAxis::Vertical),
                                        ),
                                ),
                        ),
                )
                .child({
                    let show_test = self.environment.browser.is_none()
                        && form.profile_id.is_none()
                        && !remote_to_remote;
                    let right_cluster = div()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(self.render_connection_button(
                            self.i18n.t("ssh.form.cancel"),
                            false,
                            ConnectionButtonAction::Cancel,
                            false,
                            cx,
                        ))
                        .when(form.profile_id.is_none(), |footer| {
                            footer
                                .child(self.render_connection_button(
                                    self.i18n.t("ssh.form.save"),
                                    false,
                                    ConnectionButtonAction::Save,
                                    primary_disabled,
                                    cx,
                                ))
                                .child(if remote_to_remote {
                                    self.render_connection_button(
                                        self.i18n.t("sftp.standalone.save_and_open"),
                                        true,
                                        ConnectionButtonAction::SaveAndConnect,
                                        primary_disabled,
                                        cx,
                                    )
                                } else {
                                    self.render_split_primary_action_button(
                                        self.i18n.t("sftp.standalone.save_and_open"),
                                        primary_disabled,
                                        cx,
                                    )
                                })
                        })
                        .when(form.profile_id.is_some(), |footer| {
                            footer.child(self.render_connection_button(
                                self.i18n.t("sessionManager.edit_properties.save"),
                                true,
                                ConnectionButtonAction::Save,
                                primary_disabled,
                                cx,
                            ))
                        });
                    modal_footer(&self.tokens)
                        .flex_none()
                        .h_auto()
                        .min_h(px(self.tokens.metrics.modal_footer_height))
                        .flex_wrap()
                        .bg(rgb(theme.bg_panel))
                        .px(px(studio_layout.content_padding_x))
                        .py(px(13.0))
                        .when(show_test, |footer| footer.justify_between())
                        .when(!show_test, |footer| footer.justify_end())
                        .gap(px(12.0))
                        .when(show_test, |footer| {
                            footer.child(self.render_connection_button(
                                self.i18n.t("ssh.form.test"),
                                false,
                                ConnectionButtonAction::Test,
                                primary_disabled,
                                cx,
                            ))
                        })
                        .child(right_cluster)
                }),
                form_visible,
            ))
            .into_any_element()
    }

    fn standalone_sftp_endpoint_summary(host: &str, username: &str, port: &str) -> (String, bool) {
        let host = host.trim();
        if host.is_empty() {
            return ("—".to_string(), false);
        }
        let user = username.trim();
        let port = port.trim();
        let summary = match (
            user.is_empty(),
            port.is_empty() || port == SSH_DEFAULT_PORT_TEXT,
        ) {
            (true, true) => host.to_string(),
            (true, false) => format!("{host}:{port}"),
            (false, true) => format!("{user}@{host}"),
            (false, false) => format!("{user}@{host}:{port}"),
        };
        (summary, true)
    }

    fn render_standalone_sftp_basic_page(
        &self,
        form: &StandaloneSftpModalSnapshot,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let remote_to_remote = form.transfer_mode == StandaloneSftpTransferMode::RemoteRemote;
        let total_hops = form.proxy_hops_len
            + if remote_to_remote {
                form.secondary_proxy_hops_len
            } else {
                0
            };
        let route_summary = if form.proxy_command_enabled
            || (remote_to_remote && form.secondary_proxy_command_enabled)
        {
            self.i18n.t("ssh.form.proxy_command")
        } else if total_hops > 0 {
            self.i18n
                .t("ssh.form.proxy_chain_count")
                .replace("{{count}}", &total_hops.to_string())
                .trim_end_matches(['.', '。'])
                .to_string()
        } else if remote_to_remote
            && form.secondary_upstream_proxy_policy != form.upstream_proxy_policy
        {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            match form.upstream_proxy_policy {
                NewConnectionUpstreamProxyPolicy::UseGlobal => {
                    self.i18n.t("modals.upstream_proxy.use_global")
                }
                NewConnectionUpstreamProxyPolicy::Direct => {
                    self.i18n.t("modals.upstream_proxy.direct")
                }
                NewConnectionUpstreamProxyPolicy::Custom => {
                    self.i18n.t("modals.upstream_proxy.custom")
                }
            }
        };
        let notes_trimmed = form.notes.trim();
        let notes_highlighted = !notes_trimmed.is_empty() || form.appearance_customized;
        let notes_summary = if !notes_trimmed.is_empty() {
            notes_trimmed
                .lines()
                .next()
                .unwrap_or(notes_trimmed)
                .to_string()
        } else if form.appearance_customized {
            self.i18n.t("ssh.form.ssh_algorithms_custom")
        } else {
            self.i18n.t("ssh.form.ssh_algorithms_default")
        };

        let (primary_summary, primary_set) =
            Self::standalone_sftp_endpoint_summary(&form.host, &form.username, &form.port);
        let mut summary_rows = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(self.form_hairline())
            .child(self.render_connection_disclosure_row(
                "standalone-sftp-summary-Primary",
                LucideIcon::Server,
                self.i18n.t(if remote_to_remote {
                    "sftp.standalone.remote_host_1"
                } else {
                    "sftp.standalone.remote_host"
                }),
                primary_summary,
                primary_set,
                ConnectionFormDrawer::StandaloneSftpPrimary,
                cx,
            ));
        if remote_to_remote {
            let (secondary_summary, secondary_set) = Self::standalone_sftp_endpoint_summary(
                &form.secondary_host,
                &form.secondary_username,
                &form.secondary_port,
            );
            summary_rows = summary_rows.child(self.form_hairline()).child(
                self.render_connection_disclosure_row(
                    "standalone-sftp-summary-Secondary",
                    LucideIcon::Server,
                    self.i18n.t("sftp.standalone.remote_host_2"),
                    secondary_summary,
                    secondary_set,
                    ConnectionFormDrawer::StandaloneSftpSecondary,
                    cx,
                ),
            );
        }
        summary_rows = summary_rows
            .when(self.environment.browser.is_none(), |row| row.child(self.form_hairline())
            .child(self.render_connection_disclosure_row(
                "standalone-sftp-summary-Route",
                LucideIcon::Network,
                self.i18n.t("ssh.form.connection_route"),
                route_summary,
                form.route_customized,
                ConnectionFormDrawer::Route,
                cx,
            )))
            .child(self.form_hairline())
            .child(self.render_connection_disclosure_row(
                "standalone-sftp-summary-Notes",
                LucideIcon::Sparkles,
                self.i18n.t("ssh.form.appearance"),
                notes_summary,
                notes_highlighted,
                ConnectionFormDrawer::Notes,
                cx,
            ));

        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(self.render_identity_row(
                self.i18n.t("ssh.form.name"),
                &form.name,
                self.i18n.t("ssh.form.name_placeholder"),
                NewConnectionField::Name,
                Some((self.i18n.t("ssh.form.group"), &form.group)),
                true,
                layout,
                cx,
            ))
            .child(self.form_hairline())
            .when(self.environment.browser.is_some(), |col| col.child(self.render_connection_hint(self.i18n.t("ssh.form.demo_connection_hint"))))
            .child(self.render_standalone_sftp_transfer_mode(form.transfer_mode, cx))
            .child(summary_rows)
            .when_some(form.error.clone(), |content, error| {
                content.child(self.render_prompt_feedback_box(error, form.feedback_success))
            })
            .into_any_element()
    }

    fn render_standalone_sftp_endpoint_page(
        &self,
        form: &StandaloneSftpModalSnapshot,
        secondary: bool,
        numbered: bool,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                self.render_standalone_sftp_endpoint_fields(form, secondary, numbered, layout, cx),
            )
            .when_some(form.error.clone(), |content, error| {
                content.child(self.render_prompt_feedback_box(error, form.feedback_success))
            })
            .into_any_element()
    }

    fn render_standalone_sftp_route_page(
        &self,
        form: &StandaloneSftpModalSnapshot,
        remote_to_remote: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut page = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap));
        if remote_to_remote {
            page = page.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("sftp.standalone.remote_host_1")),
            );
        }
        page = page
            .child(self.render_proxy_command_section(
                form.proxy_command_enabled,
                form.proxy_command.as_str(),
                form.proxy_command_configured,
                false,
                cx,
            ))
            .when(!form.proxy_command_enabled, |route| {
                route
                    .child(self.render_upstream_proxy_policy_section(false, cx))
                    .child(self.render_proxy_chain_section(false, cx))
            });

        if remote_to_remote {
            page = page
                .child(div().h(px(1.0)).bg(rgb(theme.border)))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_heading))
                        .child(self.i18n.t("sftp.standalone.remote_host_2")),
                )
                .child(self.render_proxy_command_section(
                    form.secondary_proxy_command_enabled,
                    form.secondary_proxy_command.as_str(),
                    form.secondary_proxy_command_configured,
                    true,
                    cx,
                ))
                .when(!form.secondary_proxy_command_enabled, |route| {
                    route
                        .child(self.render_upstream_proxy_policy_section(true, cx))
                        .child(self.render_proxy_chain_section(true, cx))
                });
        }
        page.into_any_element()
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
                self.render_sunken_segmented_tab(
                    SharedString::from(format!("standalone-sftp-mode-{mode:?}")),
                    self.i18n.t(label),
                    mode == selected,
                )
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
                self.render_sunken_segmented_track().children(tabs),
            ))
            .child(self.render_connection_hint(self.i18n.t(match selected {
                StandaloneSftpTransferMode::LocalRemote => "sftp.standalone.mode_local_remote_hint",
                StandaloneSftpTransferMode::RemoteRemote => {
                    "sftp.standalone.mode_remote_remote_hint"
                }
            })))
            .into_any_element()
    }

    fn render_standalone_sftp_endpoint_fields(
        &self,
        form: &StandaloneSftpModalSnapshot,
        secondary: bool,
        numbered: bool,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
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
        let title_key = if numbered {
            if secondary {
                "sftp.standalone.remote_host_2"
            } else {
                "sftp.standalone.remote_host_1"
            }
        } else {
            "sftp.standalone.remote_host"
        };
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
            .flex()
            .flex_col()
            .gap(px(14.0))
            .when(numbered, |section| {
                section.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_heading))
                        .child(self.i18n.t(title_key)),
                )
            })
            .child(self.render_unified_address_bar_with_fields(
                username_field,
                username,
                "root".to_string(),
                host_field,
                host,
                self.i18n.t("sftp.standalone.host_placeholder"),
                port_field,
                port,
                SSH_DEFAULT_PORT_TEXT.to_string(),
                layout.content_width,
                cx,
            ))
            .when(self.environment.browser.is_none(), |col| col.child(self.form_hairline())
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
                if secondary {
                    form.secondary_empty_password
                } else {
                    form.empty_password
                },
                cx,
            )))
            .child(self.form_hairline())
            .child(options)
            .into_any_element()
    }
}
