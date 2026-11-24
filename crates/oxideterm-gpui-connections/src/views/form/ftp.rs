use super::*;

impl ConnectionFormView {
    pub(super) fn render_ftp_form_branch(
        &self,
        active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
        layout: composer::StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::form_state::ConnectionFormDrawer;
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return div().into_any_element();
        };
        let (
            name,
            host,
            port,
            username,
            group,
            notes,
            path,
            timeout,
            tls,
            save_password,
            upstream_proxy_policy,
            appearance_customized,
        ) = (
            form.name.clone(),
            form.host.clone(),
            form.port.clone(),
            form.username.clone(),
            form.group.clone(),
            form.notes.clone(),
            form.sftp_initial_remote_path.clone(),
            form.connect_timeout_seconds_text.clone(),
            form.ftp_tls,
            form.save_password,
            form.upstream_proxy_policy,
            !form.icon.trim().is_empty()
                || !form.color.trim().is_empty()
                || !form.icon_background_color.trim().is_empty(),
        );
        let theme = self.tokens.ui;
        match active_drawer {
            Some(ConnectionFormDrawer::FtpOptions) => {
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
                            .child(self.i18n.t("modals.new_connection.ftp_timeout")),
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
                                    &timeout,
                                    "30".to_string(),
                                    NewConnectionField::ConnectTimeoutSeconds,
                                    cx,
                                ),
                            ))
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
                    .gap(px(12.0))
                    .child(self.render_connection_field(
                        self.i18n.t("modals.new_connection.ftp_initial_path"),
                        &path,
                        "/".into(),
                        NewConnectionField::InitialRemotePath,
                        false,
                        cx,
                    ))
                    .child(self.form_hairline())
                    .child(timeout_row)
                    .child(self.form_hairline())
                    .child(self.render_upstream_proxy_policy_section(false, cx))
                    .into_any_element()
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
                let save_switch = self.render_connection_switch_row(
                    "ftp-save-password",
                    self.i18n.t("ssh.form.save_password"),
                    save_password,
                    false,
                    |form| form.save_password = !form.save_password,
                    cx,
                );
                let tls_switch = self.render_connection_switch_row(
                    "ftp-tls",
                    self.i18n.t("modals.new_connection.ftp_tls"),
                    tls,
                    false,
                    |form| form.ftp_tls = !form.ftp_tls,
                    cx,
                );
                let switches = if layout.compact_content {
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(save_switch)
                        .child(self.form_hairline())
                        .child(tls_switch)
                } else {
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(18.0))
                        .child(div().flex_1().min_w_0().child(save_switch))
                        .child(div().flex_1().min_w_0().child(tls_switch))
                };
                let path_trimmed = path.trim();
                let route_highlighted = !path_trimmed.is_empty()
                    || upstream_proxy_policy != NewConnectionUpstreamProxyPolicy::UseGlobal;
                let route_summary = if !path_trimmed.is_empty() {
                    path_trimmed.to_string()
                } else {
                    match upstream_proxy_policy {
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
                    .child(self.render_unified_address_bar_with_fields(
                        NewConnectionField::Username,
                        &username,
                        self.i18n.t("ssh.form.username"),
                        NewConnectionField::Host,
                        &host,
                        self.i18n.t("ssh.form.host_placeholder"),
                        NewConnectionField::Port,
                        &port,
                        "21".to_string(),
                        layout.content_width,
                        cx,
                    ))
                    .child(self.form_hairline())
                    .child(self.render_connection_secret_field(
                        self.i18n.t("ssh.form.password"),
                        self.i18n.t("ssh.form.password"),
                        NewConnectionField::Password,
                        cx,
                    ))
                    .child(switches)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(self.form_hairline())
                            .child(self.render_connection_disclosure_row(
                                "ftp-summary-FtpOptions",
                                LucideIcon::Network,
                                self.i18n.t("ssh.form.connection_route"),
                                route_summary,
                                route_highlighted,
                                ConnectionFormDrawer::FtpOptions,
                                cx,
                            ))
                            .child(self.form_hairline())
                            .child(self.render_connection_disclosure_row(
                                "ftp-summary-Notes",
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
    }
}
