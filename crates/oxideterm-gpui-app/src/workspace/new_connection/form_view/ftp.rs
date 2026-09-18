use super::*;

impl WorkspaceApp {
    pub(super) fn render_ftp_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .child(
                div()
                    .flex()
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
                            "21".into(),
                            NewConnectionField::Port,
                            false,
                            cx,
                        ),
                    )),
            )
            .child(self.render_connection_group_select(self.i18n.t("ssh.form.group"), &group, cx))
            .child(self.render_connection_notes_fields(&notes, cx))
            .into_any_element();
        let authentication = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_field(
                self.i18n.t("ssh.form.username"),
                &username,
                self.i18n.t("ssh.form.username"),
                NewConnectionField::Username,
                false,
                cx,
            ))
            .child(self.render_connection_secret_field(
                self.i18n.t("ssh.form.password"),
                self.i18n.t("ssh.form.password"),
                NewConnectionField::Password,
                cx,
            ))
            .child(self.render_connection_checkbox(
                self.i18n.t("ssh.form.save_password"),
                save_password,
                |form| form.save_password = !form.save_password,
                cx,
            ))
            .child(self.render_connection_checkbox(
                self.i18n.t("modals.new_connection.ftp_tls"),
                tls,
                |form| form.ftp_tls = !form.ftp_tls,
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
            .into_any_element()
    }
}
