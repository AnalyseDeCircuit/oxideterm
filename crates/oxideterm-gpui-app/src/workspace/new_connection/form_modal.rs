use gpui::StatefulInteractiveElement;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_new_connection_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let mode = new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        );
        let prompt_mode = mode == super::form_state::NewConnectionFormMode::SavedConnectionPrompt;
        let duplicate_mode = mode == super::form_state::NewConnectionFormMode::DuplicateTemplate;
        let edit_properties_mode = mode.submits_saved_connection_properties();
        let drill_down_mode = self.drill_down_parent_node_id.is_some();
        let raw_tcp_edit_mode = self.editing_raw_tcp_profile_id.is_some()
            && !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::RawTcp;
        let raw_udp_edit_mode = self.editing_raw_udp_profile_id.is_some()
            && !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::RawUdp;
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let serial_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::Serial;
        let telnet_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::Telnet;
        let raw_tcp_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::RawTcp;
        let raw_udp_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && form.transport == NewConnectionTransport::RawUdp;
        let remote_desktop_protocol = if !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
        {
            match form.transport {
                NewConnectionTransport::Rdp => {
                    Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
                }
                NewConnectionTransport::Vnc => {
                    Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc)
                }
                _ => None,
            }
        } else {
            None
        };
        let local_transport_mode = serial_mode || telnet_mode || raw_tcp_mode || raw_udp_mode;
        let remote_desktop_mode = remote_desktop_protocol.is_some();
        let ssh_submission_mode = !local_transport_mode && !remote_desktop_mode;
        let shows_transport_selector = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !drill_down_mode
            && !raw_tcp_edit_mode
            && !raw_udp_edit_mode;
        let title = if drill_down_mode {
            self.i18n.t("ssh.drill_down.title")
        } else if prompt_mode {
            self.i18n
                .t("sessionManager.connect_prompt.title")
                .replace("{{name}}", &form.name)
        } else if duplicate_mode {
            self.i18n.t("sessionManager.edit_properties.duplicate_title")
        } else if raw_tcp_edit_mode {
            self.i18n.t("sessionManager.edit_properties.title")
        } else if edit_properties_mode {
            self.i18n.t("sessionManager.edit_properties.title")
        } else {
            self.i18n.t("ssh.form.title")
        };
        let description = if drill_down_mode {
            let parent_host = self
                .drill_down_parent_node_id
                .as_ref()
                .and_then(|node_id| self.ssh_nodes.get(node_id))
                .map(|node| node.title.clone())
                .unwrap_or_default();
            self.i18n
                .t("ssh.drill_down.description")
                .replace("{{host}}", &parent_host)
                .replace("<host>", "")
                .replace("</host>", "")
        } else if prompt_mode {
            format!("{}@{}:{}", form.username, form.host, form.port)
        } else if duplicate_mode {
            self.i18n
                .t("sessionManager.edit_properties.duplicate_description")
        } else if raw_tcp_edit_mode {
            self.i18n.t("sessionManager.edit_properties.description")
        } else if edit_properties_mode {
            self.i18n.t("sessionManager.edit_properties.description")
        } else if telnet_mode {
            self.i18n.t("modals.new_connection.telnet_description")
        } else if raw_tcp_mode {
            self.i18n.t("modals.new_connection.raw_tcp_description")
        } else if serial_mode {
            self.i18n.t("modals.new_connection.serial_description")
        } else if remote_desktop_protocol == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
        {
            self.i18n.t("modals.new_connection.rdp_description")
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc)
        {
            self.i18n.t("modals.new_connection.vnc_description")
        } else {
            self.i18n.t("ssh.form.subtitle")
        };
        let has_required_fields = if serial_mode {
            !form.serial_port_path.trim().is_empty()
                && form
                    .serial_baud_rate
                    .trim()
                    .parse::<u32>()
                    .is_ok_and(|baud| baud > 0)
        } else if telnet_mode {
            !form.host.trim().is_empty() && form.port.trim().parse::<u16>().is_ok()
        } else if raw_tcp_mode {
            !form.host.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
        {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && !form.password.is_empty()
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else if remote_desktop_mode {
            !form.host.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok()
        };
        let primary_disabled = form.pending || !has_required_fields;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    // Tauri NewConnectionModal is a Radix Dialog; overlay
                    // pointer-down calls onOpenChange(false), which closes and
                    // restores focus to the active pane in native.
                    this.close_new_connection_form(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                modal_container(&self.tokens)
                .w(px(if drill_down_mode {
                    TAURI_DRILL_DOWN_MODAL_WIDTH
                } else if prompt_mode || edit_properties_mode {
                    TAURI_EDIT_MODAL_WIDTH
                } else if shows_transport_selector {
                    self.tokens.metrics.modal_width
                        + NEW_CONNECTION_TYPE_SIDEBAR_WIDTH
                        + self.tokens.metrics.modal_section_gap
                } else {
                    self.tokens.metrics.modal_width
                }))
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(modal_header(&self.tokens, title, description))
                .child(
                    modal_body(&self.tokens)
                        .flex_1()
                        .min_h(px(0.0))
                        .child(
                            div()
                                .size_full()
                                .min_h(px(0.0))
                                .flex()
                                .gap(px(self.tokens.metrics.modal_section_gap))
                                .when(shows_transport_selector, |content| {
                                    content.child(self.render_transport_selector(cx))
                                })
                                .child(
                                    div()
                                        .id("new-connection-modal-form-scroll")
                                        .flex_1()
                                        .min_h(px(0.0))
                                        .min_w(px(0.0))
                                        .selectable_overflow_y_scroll(
                                            &self.selectable_text_scroll_handle(
                                                "new-connection-modal-form-scroll",
                                            ),
                                        )
                                        .on_scroll_wheel(cx.listener(
                                            |this, _event, _window, cx| {
                                                // Tauri/Radix closes select content when the modal
                                                // scroll body moves its trigger. Native caches the
                                                // trigger anchor explicitly, so clear both popup
                                                // ownership and the stale group-select bounds here.
                                                let had_open_select =
                                                    browser_behavior::close_browser_trigger_select_on_container_scroll(
                                                        &mut this.open_new_connection_select,
                                                        &mut this.new_connection_select_focus_origin,
                                                    );
                                                this.clear_new_connection_select_anchor();
                                                if had_open_select {
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(
                                            div()
                                        .flex()
                                        .flex_col()
                                        .min_w(px(0.0))
                                        .gap(px(self.tokens.metrics.modal_section_gap))
                                        .when(serial_mode, |content| {
                                            content.child(self.render_serial_form_branch(cx))
                                        })
                                        .when(telnet_mode, |content| {
                                            content.child(self.render_telnet_form_branch(cx))
                                        })
                                        .when(raw_tcp_mode, |content| {
                                            content.child(self.render_raw_tcp_form_branch(cx))
                                        })
                                        .when(raw_udp_mode, |content| {
                                            content.child(self.render_raw_udp_form_branch(cx))
                                        })
                                        .when_some(remote_desktop_protocol, |content, protocol| {
                                            content
                                                .child(self.render_remote_desktop_form_branch(protocol, cx))
                                        })
                                        .when(
                                            !serial_mode && !telnet_mode && !raw_udp_mode && !remote_desktop_mode,
                                            |content| {
                                                content
                                .when(!prompt_mode && !drill_down_mode, |content| {
                                    content
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.name"),
                                            &form.name,
                                            self.i18n.t("ssh.form.name_placeholder"),
                                            NewConnectionField::Name,
                                            false,
                                            cx,
                                        ))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .gap(px(self.tokens.metrics.form_host_port_gap))
                                                .child(div().flex_1().child(
                                                    self.render_connection_field(
                                                        self.i18n.t("ssh.form.host"),
                                                        &form.host,
                                                        self.i18n.t("ssh.form.host_placeholder"),
                                                        NewConnectionField::Host,
                                                        false,
                                                        cx,
                                                    ),
                                                ))
                                                .child(
                                                    div()
                                                        .w(px(self.tokens.metrics.form_port_width))
                                                        .child(self.render_connection_field(
                                                            self.i18n.t("ssh.form.port"),
                                                            &form.port,
                                                            SSH_DEFAULT_PORT_TEXT.to_string(),
                                                            NewConnectionField::Port,
                                                            false,
                                                            cx,
                                                        )),
                                                ),
                                        )
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.username"),
                                            &form.username,
                                            "root".to_string(),
                                            NewConnectionField::Username,
                                            false,
                                            cx,
                                        ))
                                })
                                .when(drill_down_mode, |content| {
                                    content
                                        .child(self.render_drill_saved_next_hop_picker(cx))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .gap(px(self.tokens.metrics.form_host_port_gap))
                                                .child(div().flex_1().child(
                                                    self.render_connection_field(
                                                        self.i18n.t("ssh.drill_down.target_host"),
                                                        &form.host,
                                                        self.i18n
                                                            .t("ssh.drill_down.target_host_placeholder"),
                                                        NewConnectionField::Host,
                                                        false,
                                                        cx,
                                                    ),
                                                ))
                                                .child(
                                                    div()
                                                        .w(px(self.tokens.metrics.form_port_width))
                                                        .child(self.render_connection_field(
                                                            self.i18n.t("ssh.drill_down.port"),
                                                            &form.port,
                                                            SSH_DEFAULT_PORT_TEXT.to_string(),
                                                            NewConnectionField::Port,
                                                            false,
                                                            cx,
                                                        )),
                                                ),
                                        )
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.drill_down.username"),
                                            &form.username,
                                            self.i18n.t("ssh.drill_down.username_placeholder"),
                                            NewConnectionField::Username,
                                            false,
                                            cx,
                                        ))
                                })
                                .when_some(
                                    if prompt_mode {
                                        form.error.clone()
                                    } else {
                                        None
                                    },
                                    |content, error| {
                                        content.child(self.render_prompt_error_box(error))
                                    },
                                )
                                .child(if prompt_mode {
                                    self.render_prompt_auth_radios(form.auth_tab, cx)
                                } else if drill_down_mode {
                                    self.render_drill_auth_tabs(form.auth_tab, cx)
                                } else {
                                    self.render_auth_tabs(
                                        form.auth_tab,
                                        edit_properties_mode,
                                        cx,
                                    )
                                })
                                .when(form.auth_tab == SshAuthTab::Password, |content| {
                                    if edit_properties_mode {
                                        content
                                            .child(self.render_edit_saved_password_field(form, cx))
                                            .child(self.render_connection_hint(
                                                self.i18n.t(
                                                    "sessionManager.edit_properties.password_hint",
                                                ),
                                            ))
                                            .when_some(
                                                form.password_error.clone(),
                                                |content, error| {
                                                    content.child(
                                                        div()
                                                            .text_size(px(self
                                                                .tokens
                                                                .metrics
                                                                .ui_text_xs))
                                                            .text_color(rgb(self.tokens.ui.error))
                                                            .child(error),
                                                    )
                                                },
                                            )
                                    } else if prompt_mode {
                                        content.child(self.render_connection_field(
                                            self.i18n.t("ssh.form.password"),
                                            &form.password,
                                            String::new(),
                                            NewConnectionField::Password,
                                            true,
                                            cx,
                                        ))
                                    } else if drill_down_mode {
                                        content.child(self.render_connection_field(
                                            self.i18n.t("ssh.drill_down.password"),
                                            &form.password,
                                            String::new(),
                                            NewConnectionField::Password,
                                            true,
                                            cx,
                                        ))
                                    } else {
                                        content
                                            .child(self.render_connection_field(
                                                self.i18n.t("ssh.form.password"),
                                                &form.password,
                                                String::new(),
                                                NewConnectionField::Password,
                                                true,
                                                cx,
                                            ))
                                            .child(self.render_connection_checkbox(
                                                self.i18n.t("ssh.form.save_password"),
                                                form.save_password,
                                                |form| form.save_password = !form.save_password,
                                                cx,
                                            ))
                                    }
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::DefaultKey
                                        && !prompt_mode
                                        && !edit_properties_mode,
                                    |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.default_key_desc"),
                                            ))
                                            .child(self.render_connection_field(
                                                self.i18n.t("ssh.form.passphrase"),
                                                &form.passphrase,
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                true,
                                                cx,
                                            ))
                                    },
                                )
                                .when(
                                    form.auth_tab == SshAuthTab::SshKey
                                        || ((prompt_mode || edit_properties_mode)
                                            && form.auth_tab == SshAuthTab::DefaultKey),
                                    |content| {
                                        let key_label = if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.key_path")
                                        } else if edit_properties_mode {
                                            self.i18n.t("sessionManager.edit_properties.key_path")
                                        } else {
                                            self.i18n.t("ssh.form.key_file")
                                        };
                                        let key_placeholder = if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.key_path_placeholder")
                                        } else {
                                            "~/.ssh/id_ed25519".to_string()
                                        };
                                        let key_field = if prompt_mode {
                                            self.render_connection_field(
                                                key_label,
                                                &form.key_path,
                                                key_placeholder.clone(),
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                key_label,
                                                &form.key_path,
                                                key_placeholder,
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        };
                                        content
                                            .child(key_field)
                                            .child(self.render_connection_field(
                                                if drill_down_mode {
                                                    self.i18n.t("ssh.drill_down.passphrase")
                                                } else {
                                                    self.i18n.t("ssh.form.passphrase")
                                                },
                                                &form.passphrase,
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                true,
                                                cx,
                                            ))
                                            .when(edit_properties_mode, |content| {
                                                content.child(self.render_connection_hint(
                                                    self.i18n.t(
                                                        "sessionManager.edit_properties.passphrase_hint",
                                                    ),
                                                ))
                                            })
                                    },
                                )
                                .when(form.auth_tab == SshAuthTab::ManagedKey, |content| {
                                    content
                                        .child(self.render_managed_key_select(
                                            self.i18n.t("ssh.form.managed_key"),
                                            &form.managed_key_id,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            &form.passphrase,
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            true,
                                            cx,
                                        ))
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.managed_key_hint"),
                                        ))
                                })
                                .when(form.auth_tab == SshAuthTab::Certificate, |content| {
                                    let content = if prompt_mode {
                                        content
                                    } else {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.certificate_note"),
                                        ))
                                    };
                                    content
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        })
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                cx,
                                            )
                                        })
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            &form.passphrase,
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            true,
                                            cx,
                                        ))
                                        .when(edit_properties_mode, |content| {
                                            content.child(self.render_connection_hint(
                                                self.i18n.t(
                                                    "sessionManager.edit_properties.passphrase_hint",
                                                ),
                                            ))
                                        })
                                })
                                .when(form.auth_tab == SshAuthTab::Agent, |content| {
                                    let content = content
                                        .child(self.render_connection_hint(if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.agent_desc")
                                        } else {
                                            self.i18n.t("ssh.form.agent_desc")
                                        }))
                                        .when(!drill_down_mode && !prompt_mode, |content| {
                                            content
                                                .child(self.render_agent_status(
                                                    form.agent_available,
                                                ))
                                                .child(self.render_connection_hint(
                                                    self.i18n.t("ssh.form.agent_hint"),
                                                ))
                                        });
                                    if drill_down_mode {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.drill_down.agent_hint"),
                                        ))
                                    } else {
                                        content
                                    }
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::TwoFactor
                                        && !prompt_mode
                                        && !edit_properties_mode,
                                    |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.two_factor_desc"),
                                            ))
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.two_factor_hint"),
                                            ))
                                            .child(self.render_connection_hint_with_color(
                                                self.i18n.t("ssh.form.two_factor_warning"),
                                                self.tokens.ui.warning,
                                            ))
                                    },
                                )
                                .when(!drill_down_mode, |content| {
                                    content.child(self.render_connection_group_select(
                                        if edit_properties_mode {
                                            self.i18n.t("sessionManager.edit_properties.group")
                                        } else {
                                            self.i18n.t("ssh.form.group")
                                        },
                                        &form.group,
                                        cx,
                                    ))
                                })
                                .when(edit_properties_mode, |content| {
                                    content
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.post_connect_command"),
                                            &form.post_connect_command,
                                            self.i18n
                                                .t("ssh.form.post_connect_command_placeholder"),
                                            NewConnectionField::PostConnectCommand,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.post_connect_command_hint"),
                                        ))
                                        .child(self.render_upstream_proxy_policy_section(form, cx))
                                        .child(self.render_edit_color_field(&form.color, cx))
                                })
                                .when(!prompt_mode && !edit_properties_mode, |content| {
                                    content
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(self.tokens.spacing.two))
                                                .child(self.render_connection_checkbox(
                                                    self.i18n.t("ssh.form.agent_forwarding"),
                                                    form.agent_forwarding,
                                                    |form| {
                                                        form.agent_forwarding =
                                                            !form.agent_forwarding
                                                    },
                                                    cx,
                                                ))
                                                .child(
                                                        div()
                                                        .id("new-connection-agent-forwarding-help")
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
                                                        .on_mouse_move(cx.listener(
                                                            |this,
                                                             event: &MouseMoveEvent,
                                                             _window,
                                                             cx| {
                                                                this.queue_workspace_tooltip(
                                                                    "new-connection-agent-forwarding",
                                                                    this.i18n.t("ssh.form.agent_forwarding_hint"),
                                                                    f32::from(event.position.x) + 12.0,
                                                                    f32::from(event.position.y) + 16.0,
                                                                    cx,
                                                                );
                                                            },
                                                        ))
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                |this, _event, _window, cx| {
                                                                    this.clear_workspace_tooltip(
                                                                        "new-connection-agent-forwarding",
                                                                        cx,
                                                                    );
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        )
                                                        .on_hover(cx.listener(
                                                            |this, hovered: &bool, _window, cx| {
                                                                if !*hovered {
                                                                    // TooltipContent is rendered in a
                                                                    // portal, so the trigger must clear
                                                                    // ownership explicitly on leave.
                                                                    this.clear_workspace_tooltip(
                                                                        "new-connection-agent-forwarding",
                                                                        cx,
                                                                    );
                                                                }
                                                            },
                                                        )),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(self.tokens.spacing.two))
                                                .child(self.render_connection_checkbox(
                                                    self.i18n
                                                        .t("ssh.form.legacy_ssh_compatibility"),
                                                    form.legacy_ssh_compatibility,
                                                    |form| {
                                                        form.legacy_ssh_compatibility =
                                                            !form.legacy_ssh_compatibility
                                                    },
                                                    cx,
                                                ))
                                                .child(
                                                    div()
                                                        .id(
                                                            "new-connection-legacy-ssh-compatibility-help",
                                                        )
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
                                                        .on_mouse_move(cx.listener(
                                                            |this,
                                                             event: &MouseMoveEvent,
                                                             _window,
                                                             cx| {
                                                                this.queue_workspace_tooltip(
                                                                    "new-connection-legacy-ssh-compatibility",
                                                                    this.i18n.t("ssh.form.legacy_ssh_compatibility_hint"),
                                                                    f32::from(event.position.x) + 12.0,
                                                                    f32::from(event.position.y) + 16.0,
                                                                    cx,
                                                                );
                                                            },
                                                        ))
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                |this, _event, _window, cx| {
                                                                    this.clear_workspace_tooltip(
                                                                        "new-connection-legacy-ssh-compatibility",
                                                                        cx,
                                                                    );
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        )
                                                        .on_hover(cx.listener(
                                                            |this, hovered: &bool, _window, cx| {
                                                                if !*hovered {
                                                                    // TooltipContent is rendered in a
                                                                    // portal, so the trigger must clear
                                                                    // ownership explicitly on leave.
                                                                    this.clear_workspace_tooltip(
                                                                        "new-connection-legacy-ssh-compatibility",
                                                                        cx,
                                                                    );
                                                                }
                                                            },
                                                        )),
                                                ),
                                        )
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.post_connect_command"),
                                            &form.post_connect_command,
                                            self.i18n
                                                .t("ssh.form.post_connect_command_placeholder"),
                                            NewConnectionField::PostConnectCommand,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.post_connect_command_hint"),
                                        ))
                                        .when(!drill_down_mode, |content| {
                                            content
                                                .child(self.render_upstream_proxy_policy_section(form, cx))
                                                .child(self.render_proxy_chain_section(cx))
                                        })
                                })
                                    })
                                ),
                        )
                        )
                        .when_some(
                            if prompt_mode {
                                None
                            } else {
                                form.error.clone()
                            },
                            |content, error| {
                                content.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.error))
                                        .child(error),
                                )
                            },
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
                        .when(
                            !edit_properties_mode
                                && self.saved_connection_prompt_action.is_none()
                                && !drill_down_mode
                                && ssh_submission_mode,
                            |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("ssh.form.test"),
                                    false,
                                    ConnectionButtonAction::Test,
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            !edit_properties_mode
                                && self.saved_connection_prompt_action.is_none()
                                && !remote_desktop_mode
                                && !raw_tcp_edit_mode,
                            |footer| {
                                footer
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save"),
                                        false,
                                        ConnectionButtonAction::Save,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_connection_button(
                                        if local_transport_mode {
                                            self.i18n.t("modals.new_connection.local_open")
                                        } else if drill_down_mode {
                                            self.i18n.t("ssh.drill_down.connect")
                                        } else {
                                            self.i18n.t("ssh.form.connect")
                                        },
                                        false,
                                        ConnectionButtonAction::Connect,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_connection_button(
                                        if local_transport_mode {
                                            self.i18n.t("modals.new_connection.local_save_and_open")
                                        } else if form.pending && drill_down_mode {
                                            self.i18n.t("ssh.drill_down.connecting")
                                        } else {
                                            self.i18n.t("ssh.form.save_and_connect")
                                        },
                                        true,
                                        ConnectionButtonAction::SaveAndConnect,
                                        primary_disabled,
                                        cx,
                                    ))
                            },
                        )
                        .when(raw_tcp_edit_mode, |footer| {
                            footer.child(self.render_connection_button(
                                self.i18n.t("sessionManager.edit_properties.save"),
                                true,
                                ConnectionButtonAction::Save,
                                primary_disabled,
                                cx,
                            ))
                        })
                        .when(
                            edit_properties_mode
                                || self.saved_connection_prompt_action.is_some(),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    if self.saved_connection_prompt_action
                                        == Some(SavedConnectionPromptAction::Test)
                                    {
                                        self.i18n.t("ssh.form.test")
                                    } else if self.saved_connection_prompt_action
                                        == Some(SavedConnectionPromptAction::Connect)
                                    {
                                        self.i18n.t("ssh.form.connect")
                                    } else if edit_properties_mode
                                        && self
                                            .editing_saved_connection_connect_after_save_node_id
                                            .is_some()
                                    {
                                        self.i18n
                                            .t("sessionManager.edit_properties.save_and_reconnect")
                                    } else if edit_properties_mode {
                                        self.i18n.t("sessionManager.edit_properties.save")
                                    } else {
                                        self.i18n.t("modals.new_connection.local_open")
                                    },
                                    true,
                                    if edit_properties_mode
                                        && self.saved_connection_prompt_action.is_none()
                                    {
                                        ConnectionButtonAction::Save
                                    } else {
                                        ConnectionButtonAction::Connect
                                    },
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            remote_desktop_mode
                                && !edit_properties_mode
                                && self.saved_connection_prompt_action.is_none(),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("ssh.form.connect"),
                                    true,
                                    ConnectionButtonAction::Connect,
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        ),
                ),
        )
        .into_any_element()
    }

    fn render_drill_saved_next_hop_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(parent_node_id) = self.drill_down_parent_node_id.clone() else {
            return div().into_any_element();
        };
        let parent_title = self
            .ssh_nodes
            .get(&parent_node_id)
            .map(|node| node.title.clone())
            .unwrap_or_default();
        let description = self
            .i18n
            .t("sessions.saved_next_hop.description")
            .replace("{{host}}", &parent_title);
        let connections = self.connection_store.connection_infos();
        let has_connections = !connections.is_empty();
        let mut list = div().flex().flex_col().gap(px(4.0));
        for connection in connections {
            list = list.child(self.render_drill_saved_next_hop_row(
                parent_node_id.clone(),
                connection,
                cx,
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgba((theme.bg_card << 8) | 0x66))
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sessions.saved_next_hop.title")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(description),
                    ),
            )
            .when(!has_connections, |section| {
                section.child(self.render_connection_hint(
                    self.i18n.t("sessions.saved_next_hop.empty"),
                ))
            })
            .when(has_connections, |section| {
                section.child(
                    div()
                        .id("drill-saved-next-hop-scroll")
                        .max_h(px(180.0))
                        .selectable_overflow_y_scroll(
                            &self.selectable_text_scroll_handle("drill-saved-next-hop-scroll"),
                        )
                        .child(list),
                )
            })
            .into_any_element()
    }

    fn render_drill_saved_next_hop_row(
        &self,
        parent_node_id: oxideterm_ssh::NodeId,
        connection: oxideterm_connections::ConnectionInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let connection_id = connection.id.clone();
        let detail = format!(
            "{}@{}:{}",
            connection.username, connection.host, connection.port
        );
        let proxy_hop_count = connection.proxy_chain.len();
        let proxy_badge = self
            .i18n
            .t("sessions.saved_next_hop.proxy_chain_badge")
            .replace("{{count}}", &proxy_hop_count.to_string());

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .px(px(8.0))
            .py(px(6.0))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                13.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(connection.name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(detail),
                    ),
            )
            .when(proxy_hop_count > 0, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((theme.accent << 8) | 0x1a))
                        .px(px(6.0))
                        .py(px(2.0))
                        .text_size(px(10.0))
                        .text_color(rgb(theme.accent))
                        .child(proxy_badge),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.connect_saved_connection_as_next_hop(
                        parent_node_id.clone(),
                        connection_id.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

}
