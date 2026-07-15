use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_new_connection_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // New-connection dropdowns mount and unmount with their logical state;
        // no render-only payload is retained for an exit transition.
        let select_id = self.open_new_connection_select?;
        let anchor_id = Self::new_connection_select_anchor_id(select_id);
        let anchor = self.select_anchors.get(&anchor_id).copied()?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let viewport_height = f32::from(window.viewport_size().height);
        let popup_gap = self.tokens.metrics.settings_select_popup_gap;
        let below = viewport_height - f32::from(anchor.bounds.bottom()) - popup_gap;
        let above = f32::from(anchor.bounds.top()) - popup_gap;
        let opens_above = below < self.tokens.metrics.ui_select_max_height && above > below;
        let max_height = if opens_above { above } else { below }
            .max(self.tokens.metrics.ui_control_height)
            .min(self.tokens.metrics.ui_select_max_height);

        let mut popup = select_overlay_popup_with_max_height(&self.tokens, width, max_height);
        match select_id {
            NewConnectionSelect::Group => {
                let current_group = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.group.as_str())
                    .unwrap_or_default();
                let ungrouped_label = self.connection_form_ungrouped_label();
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        ungrouped_label.clone(),
                        self.connection_form_group_is_ungrouped(current_group),
                    ),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_new_connection_select();
                        this.set_new_connection_group(ungrouped_label.clone(), cx);
                        cx.stop_propagation();
                    }),
                ));

                let groups = self.connection_form_group_options(current_group);
                for group in groups.iter().cloned() {
                    let selected = group == current_group;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, group.clone(), selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_group(group.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                if groups.is_empty() {
                    popup = popup.child(
                        div()
                            .relative()
                            .flex()
                            .w_full()
                            .items_center()
                            .rounded(px(self.tokens.radii.xs))
                            .py(px(self.tokens.metrics.ui_menu_item_padding_y))
                            .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .opacity(0.65)
                            .child(self.i18n.t("ssh.form.create_groups_hint")),
                    );
                }
            }
            NewConnectionSelect::JumpSavedConnection => {
                let selected_connection_id = self
                    .new_connection_form
                    .as_ref()
                    .and_then(|form| form.jump_server_form.as_ref())
                    .map(|jump_form| jump_form.saved_connection_id.as_str())
                    .unwrap_or_default();
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n.t("ssh.form.proxy_jump_saved_connection_custom"),
                        selected_connection_id.is_empty(),
                    ),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select();
                        this.clear_new_connection_jump_saved_connection(cx);
                        cx.stop_propagation();
                    }),
                ));
                for connection in self.connection_store.connection_infos() {
                    let selected = connection.id == selected_connection_id;
                    let connection_id = connection.id.clone();
                    let label = format!(
                        "{} · {}@{}:{}",
                        connection.name, connection.username, connection.host, connection.port
                    );
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_jump_saved_connection(
                                connection_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::KeyAuthSource | NewConnectionSelect::JumpKeyAuthSource => {
                let context = match select_id {
                    NewConnectionSelect::KeyAuthSource => self.current_main_auth_selector_context(),
                    NewConnectionSelect::JumpKeyAuthSource => AuthSelectorContext::Jump,
                    _ => unreachable!("matched only key auth source selects"),
                };
                let active_tab = self
                    .new_connection_form
                    .as_ref()
                    .and_then(|form| match select_id {
                        NewConnectionSelect::KeyAuthSource => Some(form.auth_tab),
                        NewConnectionSelect::JumpKeyAuthSource => form
                            .jump_server_form
                            .as_ref()
                            .map(|jump_form| jump_form.auth_tab),
                        _ => None,
                    })
                    .unwrap_or(SshAuthTab::SshKey);
                let active_source = Self::normalized_key_source_for_context(active_tab, context);
                for source in Self::key_auth_source_choices(context).iter().copied() {
                    let label = self.key_auth_source_label(source);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, source == active_source),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_key_auth_source(select_id, source, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::ManagedKey | NewConnectionSelect::JumpManagedKey => {
                let current_key_id = self
                    .new_connection_form
                    .as_ref()
                    .and_then(|form| match select_id {
                        NewConnectionSelect::ManagedKey => Some(form.managed_key_id.as_str()),
                        NewConnectionSelect::JumpManagedKey => form
                            .jump_server_form
                            .as_ref()
                            .map(|jump_form| jump_form.managed_key_id.as_str()),
                        _ => None,
                    })
                    .unwrap_or_default();
                for key in self.connection_store.managed_ssh_keys() {
                    let selected = key.id == current_key_id;
                    let key_id = key.id.clone();
                    let label = format!("{} · {}", key.name, key.fingerprint);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_managed_key(select_id, key_id.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::UpstreamProxyPolicy => {
                let selected = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.upstream_proxy_policy)
                    .unwrap_or(NewConnectionUpstreamProxyPolicy::UseGlobal);
                for (policy, label_key) in [
                    (
                        NewConnectionUpstreamProxyPolicy::UseGlobal,
                        "modals.upstream_proxy.use_global",
                    ),
                    (
                        NewConnectionUpstreamProxyPolicy::Direct,
                        "modals.upstream_proxy.direct",
                    ),
                    (
                        NewConnectionUpstreamProxyPolicy::Custom,
                        "modals.upstream_proxy.custom",
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), policy == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_upstream_proxy_policy(policy, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::UpstreamProxyProtocol => {
                let selected = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.upstream_proxy_protocol)
                    .unwrap_or(SavedUpstreamProxyProtocol::Socks5);
                for (protocol, label_key) in [
                    (
                        SavedUpstreamProxyProtocol::Socks5,
                        "settings_view.network.protocol_socks5",
                    ),
                    (
                        SavedUpstreamProxyProtocol::HttpConnect,
                        "settings_view.network.protocol_http_connect",
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), protocol == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_upstream_proxy_protocol(protocol, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::UpstreamProxyAuth => {
                let selected = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.upstream_proxy_auth)
                    .unwrap_or(NewConnectionUpstreamProxyAuth::None);
                for (auth, label_key) in [
                    (
                        NewConnectionUpstreamProxyAuth::None,
                        "settings_view.network.auth_none",
                    ),
                    (
                        NewConnectionUpstreamProxyAuth::Password,
                        "settings_view.network.auth_password",
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), auth == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_upstream_proxy_auth(auth, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialPort => {
                let selected_port = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.serial_port_path.as_str())
                    .unwrap_or_default();
                let ports = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.serial_ports.clone())
                    .unwrap_or_default();
                for port in ports {
                    let selected = port.port_path == selected_port;
                    let port_path = port.port_path.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, serial_port_display_label(&port), selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_serial_port(port_path.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialDataBits | NewConnectionSelect::SerialStopBits => {
                let selected = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| match select_id {
                        NewConnectionSelect::SerialDataBits => form.serial_data_bits,
                        NewConnectionSelect::SerialStopBits => form.serial_stop_bits,
                        _ => 0,
                    })
                    .unwrap_or_default();
                let choices: &[(u8, &str)] = match select_id {
                    NewConnectionSelect::SerialDataBits => {
                        &[(5, "5"), (6, "6"), (7, "7"), (8, "8")]
                    }
                    NewConnectionSelect::SerialStopBits => &[(1, "1"), (2, "2")],
                    _ => &[],
                };
                for (value, label) in choices.iter().copied() {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label.to_string(), value == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_serial_u8(select_id, value, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialParity => {
                let selected = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.serial_parity)
                    .unwrap_or(oxideterm_terminal::SerialParity::None);
                for parity in [
                    oxideterm_terminal::SerialParity::None,
                    oxideterm_terminal::SerialParity::Odd,
                    oxideterm_terminal::SerialParity::Even,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.serial_parity_label(parity),
                            parity == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_serial_parity(parity, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialFlowControl => {
                let selected = self
                    .new_connection_form
                    .as_ref()
                    .map(|form| form.serial_flow_control)
                    .unwrap_or(oxideterm_terminal::SerialFlowControl::None);
                for flow in [
                    oxideterm_terminal::SerialFlowControl::None,
                    oxideterm_terminal::SerialFlowControl::Software,
                    oxideterm_terminal::SerialFlowControl::Hardware,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.serial_flow_control_label(flow),
                            flow == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.set_new_connection_serial_flow_control(flow, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
        }

        let (anchor_corner, position, offset_y) = if opens_above {
            (
                Corner::BottomLeft,
                point(anchor.bounds.left(), anchor.bounds.top()),
                -popup_gap,
            )
        } else {
            (Corner::TopLeft, anchor.bounds.bottom_left(), popup_gap)
        };
        let popup = popup.into_any_element();

        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(anchor_corner)
                            .position(position)
                            .offset(point(px(0.0), px(offset_y)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_add_jump_server_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(jump_form) = self
            .new_connection_form
            .as_ref()
            .and_then(|form| form.jump_server_form.as_ref())
        else {
            return div().into_any_element();
        };
        let add_disabled = !jump_form.complete()
            || (jump_form.auth_tab == SshAuthTab::ManagedKey
                && jump_form.managed_key_id.trim().is_empty());
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let form_visible =
            self.jump_server_form_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri jump-server form is a Dialog child of the new
                    // connection flow; overlay clicks cancel just this subform.
                    this.begin_jump_server_form_exit(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "jump-server-form-enter",
                modal_container(&self.tokens)
                    .w(px(TAURI_JUMP_MODAL_WIDTH))
                    .max_h(px(modal_max_height))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(modal_header(
                        &self.tokens,
                        self.i18n.t("ssh.form.proxy_jump_title"),
                        String::new(),
                    ))
                    .child(
                        modal_body(&self.tokens)
                            .id("new-connection-jump-server-body-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .selectable_overflow_y_scroll(&self.selectable_text_scroll_handle(
                                "new-connection-jump-server-body-scroll",
                            ))
                            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                                // Keep native anchored selects aligned with Tauri/Radix:
                                // scrolling the modal body closes popup content tied to a moved trigger.
                                if this.open_new_connection_select.is_some() {
                                    this.close_new_connection_select();
                                    this.clear_new_connection_select_anchor();
                                    cx.notify();
                                }
                            }))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(self.render_jump_saved_connection_select(
                                &jump_form.saved_connection_id,
                                cx,
                            ))
                            .child(self.render_connection_hint(
                                if self.connection_store.connection_infos().is_empty() {
                                    self.i18n.t("ssh.form.proxy_jump_saved_connection_empty")
                                } else {
                                    self.i18n.t("ssh.form.proxy_jump_saved_connection_hint")
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_4()
                                    .child(div().flex_1().child(self.render_connection_field(
                                        self.i18n.t("ssh.form.proxy_jump_host"),
                                        &jump_form.host,
                                        self.i18n.t("ssh.form.proxy_jump_host_placeholder"),
                                        NewConnectionField::JumpHost,
                                        false,
                                        cx,
                                    )))
                                    .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                                        self.render_connection_field(
                                            self.i18n.t("ssh.form.proxy_jump_port"),
                                            &jump_form.port,
                                            "22".to_string(),
                                            NewConnectionField::JumpPort,
                                            false,
                                            cx,
                                        ),
                                    )),
                            )
                            .child(self.render_connection_field(
                                self.i18n.t("ssh.form.proxy_jump_username"),
                                &jump_form.username,
                                self.i18n.t("ssh.form.proxy_jump_username_placeholder"),
                                NewConnectionField::JumpUsername,
                                false,
                                cx,
                            ))
                            .child(self.render_connection_hint(
                                self.i18n.t("ssh.form.proxy_jump_kbi_hint"),
                            ))
                            .child(self.render_auth_selector(
                                jump_form.auth_tab,
                                AuthSelectorContext::Jump,
                                true,
                                cx,
                            ))
                            .when(jump_form.auth_tab == SshAuthTab::DefaultKey, |content| {
                                content.child(self.render_connection_hint(
                                    self.i18n.t("ssh.form.default_key_desc"),
                                ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::SshKey, |content| {
                                content
                                    .child(self.render_connection_field_with_browse(
                                        self.i18n.t("ssh.form.proxy_jump_key_path"),
                                        &jump_form.key_path,
                                        self.i18n.t("ssh.form.proxy_jump_key_path_placeholder"),
                                        NewConnectionField::JumpKeyPath,
                                        cx,
                                    ))
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.passphrase"),
                                        &jump_form.passphrase,
                                        String::new(),
                                        NewConnectionField::JumpPassphrase,
                                        true,
                                        cx,
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::ManagedKey, |content| {
                                content
                                    .child(self.render_managed_key_select(
                                        self.i18n.t("ssh.form.managed_key"),
                                        &jump_form.managed_key_id,
                                        true,
                                        cx,
                                    ))
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.passphrase"),
                                        &jump_form.passphrase,
                                        self.i18n.t("ssh.form.passphrase_placeholder"),
                                        NewConnectionField::JumpPassphrase,
                                        true,
                                        cx,
                                    ))
                                    .child(self.render_connection_hint(
                                        self.i18n.t("ssh.form.managed_key_hint"),
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::Certificate, |content| {
                                content
                                    .child(self.render_connection_field_with_browse(
                                        self.i18n.t("ssh.form.private_key"),
                                        &jump_form.key_path,
                                        self.i18n.t("ssh.form.proxy_jump_key_path_placeholder"),
                                        NewConnectionField::JumpKeyPath,
                                        cx,
                                    ))
                                    .child(self.render_connection_field_with_browse(
                                        self.i18n.t("ssh.form.certificate"),
                                        &jump_form.cert_path,
                                        "~/.ssh/id_ed25519-cert.pub".to_string(),
                                        NewConnectionField::JumpCertPath,
                                        cx,
                                    ))
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.passphrase"),
                                        &jump_form.passphrase,
                                        String::new(),
                                        NewConnectionField::JumpPassphrase,
                                        true,
                                        cx,
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::Password, |content| {
                                content.child(self.render_connection_field(
                                    self.i18n.t("ssh.form.password"),
                                    &jump_form.password,
                                    String::new(),
                                    NewConnectionField::JumpPassword,
                                    true,
                                    cx,
                                ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::Agent, |content| {
                                content.child(self.render_connection_hint(
                                    self.i18n.t("ssh.form.proxy_jump_agent_desc"),
                                ))
                            })
                            .child(self.render_connection_checkbox(
                                self.i18n.t("ssh.form.agent_forwarding"),
                                jump_form.agent_forwarding,
                                |form| {
                                    if let Some(jump_form) = form.jump_server_form.as_mut() {
                                        jump_form.agent_forwarding = !jump_form.agent_forwarding;
                                    }
                                },
                                cx,
                            ))
                            .child(self.render_connection_checkbox(
                                self.i18n.t("ssh.form.legacy_ssh_compatibility"),
                                jump_form.legacy_ssh_compatibility,
                                |form| {
                                    if let Some(jump_form) = form.jump_server_form.as_mut() {
                                        jump_form.legacy_ssh_compatibility =
                                            !jump_form.legacy_ssh_compatibility;
                                    }
                                },
                                cx,
                            )),
                    )
                    .child(
                        modal_footer(&self.tokens)
                            .child(self.render_jump_cancel_button(cx))
                            .child(self.render_jump_add_button(add_disabled, cx)),
                    ),
                form_visible,
            ))
            .when(!form_visible, |backdrop| {
                backdrop.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
                )
            })
            .into_any_element()
    }

    pub(super) fn begin_jump_server_form_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.begin_jump_server_form_exit_with_commit(false, cx)
    }

    fn begin_jump_server_form_exit_with_commit(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.jump_server_form_presence.begin_exit() else {
            return false;
        };
        self.jump_server_exit_commits = commit;
        if let Some(form) = self.new_connection_form.as_mut() {
            form.field_focused = false;
            form.selected_field = None;
        }
        self.ime_marked_text = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.finish_jump_server_form_exit(generation, cx);
            return true;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_jump_server_form_exit(generation, cx) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    fn finish_jump_server_form_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.jump_server_form_presence.finish_exit(generation) {
            return false;
        }
        let commit = std::mem::take(&mut self.jump_server_exit_commits);
        if commit {
            self.commit_pending_jump_server(cx);
        } else if let Some(form) = self.new_connection_form.as_mut() {
            form.jump_server_form = None;
        }
        self.jump_server_form_presence.reopen();
        true
    }

    pub(super) fn render_proxy_chain_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let (hops, expanded) = self
            .new_connection_form
            .as_ref()
            .map(|form| (form.proxy_hops.clone(), form.proxy_chain_expanded))
            .unwrap_or_default();
        let mut list = div()
            .id("new-connection-proxy-chain-scroll")
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(TAURI_PROXY_CHAIN_MAX_HEIGHT))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("new-connection-proxy-chain-scroll"),
            );
        if hops.is_empty() {
            list = list.child(
                div()
                    .py(px(24.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ssh.form.proxy_chain_empty")),
            );
        } else {
            for (index, hop) in hops.iter().cloned().enumerate() {
                list = list.child(self.render_proxy_hop_summary(index, hop, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .rounded(px(self.tokens.radii.lg))
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(TAURI_PROXY_CHAIN_SECTION_PADDING))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(TAURI_PROXY_CHAIN_HEADER_MARGIN))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.i18n.t("ssh.form.proxy_chain_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(!hops.is_empty(), |row| {
                                row.child(self.render_proxy_chain_toggle(expanded, cx))
                            })
                            .child(self.render_add_jump_button(cx)),
                    ),
            )
            .child(if expanded {
                list.into_any_element()
            } else {
                div()
                    .py(px(24.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(if hops.is_empty() {
                        self.i18n.t("ssh.form.proxy_chain_empty")
                    } else {
                        self.i18n
                            .t("ssh.form.proxy_chain_count")
                            .replace("{{count}}", &hops.len().to_string())
                    })
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_proxy_chain_toggle(&self, expanded: bool, cx: &mut Context<Self>) -> AnyElement {
        // Proxy-chain expand/collapse is an icon-only toolbar action in the
        // Tauri form. Use the shared primitive so hover and future focus state
        // stay aligned with other new-connection toolbar controls.
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            self.render_animated_chevron(
                ("proxy-chain-chevron", expanded as usize),
                expanded,
                16.0,
                rgb(self.tokens.ui.text),
            ),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.ui_button_sm_height,
                    ButtonRadius::Md,
                )
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.proxy_chain_expanded = !form.proxy_chain_expanded;
                    form.field_focused = false;
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_add_jump_button(&self, cx: &mut Context<Self>) -> AnyElement {
        // The outer "add jump" command is the same small outline action
        // pattern used by settings toolbars, so keep its chrome shared.
        self.workspace_toolbar_action_button(
            self.i18n.t("ssh.form.proxy_chain_add_jump"),
            Some(Self::render_lucide_icon(
                LucideIcon::Plus,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, window, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.jump_server_form = Some(NewConnectionProxyHop::new());
                    form.field_focused = true;
                    form.focused_field = NewConnectionField::JumpHost;
                    form.selected_field = None;
                }
                this.jump_server_form_presence.reopen();
                this.close_new_connection_select();
                this.new_connection_caret_visible = true;
                window.focus(&this.focus_handle);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_jump_add_button(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        // Jump-server editor actions mirror Tauri Button chrome. Keep add and
        // cancel on the shared toolbar primitive instead of local button_with
        // calls so disabled/focus handling can converge later.
        self.workspace_toolbar_action_button(
            self.i18n.t("ssh.form.proxy_jump_add"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Default,
                    disabled,
                    ..ButtonOptions::default()
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.add_pending_jump_server(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_jump_cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("ssh.form.cancel"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Default,
                    ..ButtonOptions::default()
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.begin_jump_server_form_exit(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_proxy_hop_summary(
        &self,
        index: usize,
        hop: NewConnectionProxyHop,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auth_label = match hop.auth_tab {
            SshAuthTab::DefaultKey => self.i18n.t("ssh.auth.default_key"),
            SshAuthTab::SshKey => self.i18n.t("ssh.auth.ssh_key"),
            SshAuthTab::ManagedKey => self.i18n.t("ssh.auth.managed_key"),
            SshAuthTab::Certificate => self.i18n.t("ssh.auth.certificate"),
            SshAuthTab::Password => self.i18n.t("ssh.auth.password"),
            SshAuthTab::Agent => self.i18n.t("ssh.auth.agent"),
            SshAuthTab::TwoFactor => self.i18n.t("ssh.auth.two_factor"),
        };
        let auth_icon = if matches!(
            hop.auth_tab,
            SshAuthTab::SshKey | SshAuthTab::DefaultKey | SshAuthTab::ManagedKey
        ) {
            LucideIcon::Key
        } else {
            LucideIcon::Lock
        };
        div()
            .relative()
            .child(
                div()
                    .absolute()
                    .left(px(TAURI_PROXY_CHAIN_NODE_SIZE / 2.0))
                    .top_0()
                    .bottom_0()
                    .w(px(TAURI_PROXY_CHAIN_CONNECTOR_THICKNESS))
                    .when(index > 0, |line| {
                        line.child(
                            div()
                                .absolute()
                                .top(px(TAURI_PROXY_CHAIN_NODE_SIZE / 2.0))
                                .w(px(TAURI_PROXY_CHAIN_LINE_WIDTH))
                                .h(px(TAURI_PROXY_CHAIN_CONNECTOR_THICKNESS))
                                .bg(rgb(self.tokens.ui.text_muted)),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .size(px(TAURI_PROXY_CHAIN_NODE_SIZE))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(self.tokens.ui.border_strong))
                            .bg(rgb(self.tokens.ui.bg))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                auth_icon,
                                16.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    ),
            )
            .child(
                div().flex().items_start().gap(px(24.0)).pl(px(48.0)).child(
                    div()
                        .flex_1()
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .rounded(px(self.tokens.radii.lg))
                        .p(px(TAURI_PROXY_CHAIN_CARD_PADDING))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(format!(
                                            "{}. {}",
                                            index + 1,
                                            self.i18n.t("ssh.form.proxy_chain_jump_server")
                                        )),
                                )
                                .child(self.render_remove_jump_button(index, cx)),
                        )
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_host"),
                            hop.host,
                            cx,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_port"),
                            hop.port,
                            cx,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_username"),
                            hop.username,
                            cx,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_auth"),
                            auth_label,
                            cx,
                        )),
                ),
            )
            .into_any_element()
    }

    fn render_proxy_hop_line(
        &self,
        label: String,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(div().text_color(rgb(self.tokens.ui.text_muted)).child(
                self.render_selectable_text_scoped(
                    "proxy-hop-label",
                    (&label, &value),
                    format!("{label}:"),
                    self.tokens.ui.text_muted,
                    cx,
                ),
            ))
            .child(div().font_weight(gpui::FontWeight::MEDIUM).child(
                self.render_selectable_text_scoped(
                    "proxy-hop-value",
                    (&label, &value),
                    value.clone(),
                    self.tokens.ui.text,
                    cx,
                ),
            ))
            .into_any_element()
    }

    fn render_remove_jump_button(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        self.workspace_icon_action_button(
            LucideIcon::Trash2,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Sm)
            },
            move |this, _event, _window, cx| {
                if let Some(form) = this.new_connection_form.as_mut()
                    && index < form.proxy_hops.len()
                {
                    form.proxy_hops.remove(index);
                }
                cx.stop_propagation();
                cx.notify();
            },
            cx,
        )
        .into_any_element()
    }

    pub(super) fn add_pending_jump_server(&mut self, cx: &mut Context<Self>) {
        let Some(jump_form) = self
            .new_connection_form
            .as_ref()
            .and_then(|form| form.jump_server_form.as_ref())
        else {
            return;
        };
        if !jump_form.complete() {
            if let Some(form) = self.new_connection_form.as_mut() {
                form.error = Some(self.i18n.t("ssh.form.proxy_jump_required"));
            }
            cx.notify();
            return;
        }
        self.begin_jump_server_form_exit_with_commit(true, cx);
    }

    fn commit_pending_jump_server(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let Some(jump_form) = form.jump_server_form.take() else {
            return;
        };
        if !jump_form.complete() {
            form.jump_server_form = Some(jump_form);
            form.error = Some(self.i18n.t("ssh.form.proxy_jump_required"));
            cx.notify();
            return;
        }
        form.proxy_hops.push(jump_form);
        if form.auth_tab == SshAuthTab::TwoFactor {
            form.auth_tab = SshAuthTab::Password;
            form.focused_field = NewConnectionField::Password;
        }
        form.proxy_chain_expanded = true;
        form.field_focused = false;
        form.selected_field = None;
        form.error = None;
        self.ime_marked_text = None;
        cx.notify();
    }
}
