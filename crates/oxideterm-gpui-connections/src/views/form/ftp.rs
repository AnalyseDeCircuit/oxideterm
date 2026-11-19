use super::*;

impl ConnectionFormView {
    pub(super) fn render_ftp_form_branch(
        &self,
        active_drawer: Option<crate::form_state::ConnectionFormDrawer>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::form_state::ConnectionFormDrawer;
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return div().into_any_element();
        };
        let (name, host, port, username, group, notes, path, timeout, tls, save_password) = (
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
        );
        match active_drawer {
            Some(ConnectionFormDrawer::FtpOptions) => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_field(
                    self.i18n.t("modals.new_connection.ftp_initial_path"),
                    &path,
                    "/".into(),
                    NewConnectionField::InitialRemotePath,
                    false,
                    cx,
                ))
                .child(self.render_connection_field(
                    self.i18n.t("modals.new_connection.ftp_timeout"),
                    &timeout,
                    "30".into(),
                    NewConnectionField::ConnectTimeoutSeconds,
                    false,
                    cx,
                ))
                .child(self.render_upstream_proxy_policy_section(false, cx))
                .into_any_element(),
            Some(ConnectionFormDrawer::Notes) => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.modal_section_gap))
                .child(self.render_connection_appearance_options(cx))
                .child(div().h(px(1.0)).bg(rgb(self.tokens.ui.border)))
                .child(self.render_connection_notes_fields(&notes, cx))
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(12.0))
                        .child(div().flex_1().min_w_0().child(self.render_connection_field(
                            self.i18n.t("ssh.form.name"),
                            &name,
                            self.i18n.t("ssh.form.name_placeholder"),
                            NewConnectionField::Name,
                            false,
                            cx,
                        )))
                        .child(div().w(px(196.0)).flex_none().child(
                            self.render_connection_group_select(
                                self.i18n.t("ssh.form.group"),
                                &group,
                                cx,
                            ),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(12.0))
                        .child(div().flex_1().min_w_0().child(self.render_connection_field(
                            self.i18n.t("ssh.form.host"),
                            &host,
                            self.i18n.t("ssh.form.host_placeholder"),
                            NewConnectionField::Host,
                            false,
                            cx,
                        )))
                        .child(div().w(px(88.0)).flex_none().child(
                            self.render_connection_field(
                                self.i18n.t("ssh.form.port"),
                                &port,
                                "21".into(),
                                NewConnectionField::Port,
                                false,
                                cx,
                            ),
                        ))
                        .child(div().w(px(156.0)).flex_none().child(
                            self.render_connection_field(
                                self.i18n.t("ssh.form.username"),
                                &username,
                                self.i18n.t("ssh.form.username"),
                                NewConnectionField::Username,
                                false,
                                cx,
                            ),
                        )),
                )
                .child(div().w_full().h(px(1.0)).bg(rgb(self.tokens.ui.border)))
                .child(self.render_connection_secret_field(
                    self.i18n.t("ssh.form.password"),
                    self.i18n.t("ssh.form.password"),
                    NewConnectionField::Password,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(20.0))
                        .child(self.render_connection_checkbox(
                            "ftp-save-password",
                            self.i18n.t("ssh.form.save_password"),
                            save_password,
                            |form| form.save_password = !form.save_password,
                            cx,
                        ))
                        .child(self.render_connection_checkbox(
                            "ftp-tls",
                            self.i18n.t("modals.new_connection.ftp_tls"),
                            tls,
                            |form| form.ftp_tls = !form.ftp_tls,
                            cx,
                        )),
                )
                .into_any_element(),
        }
    }
}
