impl WorkspaceApp {
    pub(in crate::workspace) fn render_new_connection_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.open_new_connection_select != Some(NewConnectionSelect::Group) {
            return None;
        }
        let anchor = *self
            .select_anchors
            .get(&SelectAnchorId::NewConnectionGroup)?;
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

        let current_group = self
            .new_connection_form
            .as_ref()
            .map(|form| form.group.as_str())
            .unwrap_or_default();
        let ungrouped_label = self.connection_form_ungrouped_label();
        let mut popup = select_overlay_popup_with_max_height(&self.tokens, width, max_height).child(
                select_option_action(
                    select_option(
                        &self.tokens,
                        ungrouped_label.clone(),
                        self.connection_form_group_is_ungrouped(current_group),
                    ),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_new_connection_group(ungrouped_label.clone(), cx);
                        cx.stop_propagation();
                    }),
                ),
            );

        let groups = self.connection_form_group_options(current_group);
        for group in groups.iter().cloned() {
            let selected = group == current_group;
            popup = popup.child(
                select_option_action(
                    select_option(&self.tokens, group.clone(), selected),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_new_connection_group(group.clone(), cx);
                        cx.stop_propagation();
                    }),
                ),
            );
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

        let (anchor_corner, position, offset_y) = if opens_above {
            (
                Corner::BottomLeft,
                point(anchor.bounds.left(), anchor.bounds.top()),
                -popup_gap,
            )
        } else {
            (Corner::TopLeft, anchor.bounds.bottom_left(), popup_gap)
        };

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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(jump_form) = self
            .new_connection_form
            .as_ref()
            .and_then(|form| form.jump_server_form.as_ref())
        else {
            return div().into_any_element();
        };
        let add_disabled = !jump_form.complete();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri jump-server form is a Dialog child of the new
                    // connection flow; overlay clicks cancel just this subform.
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.jump_server_form = None;
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    this.ime_marked_text = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                modal_container(&self.tokens)
                .w(px(TAURI_JUMP_MODAL_WIDTH))
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
                        .flex()
                        .flex_col()
                        .gap_4()
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
                        .child(
                            self.render_connection_hint(
                                self.i18n.t("ssh.form.proxy_jump_kbi_hint"),
                            ),
                        )
                        .child(self.render_jump_auth_tabs(jump_form.auth_tab, cx))
                        .when(jump_form.auth_tab == SshAuthTab::DefaultKey, |content| {
                            content.child(
                                self.render_connection_hint(
                                    self.i18n.t("ssh.form.default_key_desc"),
                                ),
                            )
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
                        )),
                )
                .child(
                    modal_footer(&self.tokens)
                        .child(self.render_jump_cancel_button(cx))
                        .child(self.render_jump_add_button(add_disabled, cx)),
                ),
        )
        .into_any_element()
    }

    fn render_proxy_chain_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
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
        toolbar_button(
            &self.tokens,
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
        )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.jump_server_form =
                            Some(super::form_state::NewConnectionProxyHop::new());
                        form.field_focused = true;
                        form.focused_field = NewConnectionField::JumpHost;
                        form.selected_field = None;
                    }
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
        toolbar_button(
            &self.tokens,
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
        )
        .when(!disabled, |button| {
            button.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.add_pending_jump_server(cx);
                    cx.stop_propagation();
                }),
            )
        })
        .into_any_element()
    }

    fn render_jump_cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        toolbar_button(
            &self.tokens,
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
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.jump_server_form = None;
                    form.field_focused = false;
                    form.selected_field = None;
                }
                this.ime_marked_text = None;
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_proxy_hop_summary(
        &self,
        index: usize,
        hop: super::form_state::NewConnectionProxyHop,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auth_label = match hop.auth_tab {
            SshAuthTab::DefaultKey => self.i18n.t("ssh.auth.default_key"),
            SshAuthTab::SshKey => self.i18n.t("ssh.auth.ssh_key"),
            SshAuthTab::Certificate => self.i18n.t("ssh.auth.certificate"),
            SshAuthTab::Password => self.i18n.t("ssh.auth.password"),
            SshAuthTab::Agent => self.i18n.t("ssh.auth.agent"),
            SshAuthTab::TwoFactor => self.i18n.t("ssh.auth.two_factor"),
        };
        let auth_icon = if matches!(hop.auth_tab, SshAuthTab::SshKey | SshAuthTab::DefaultKey) {
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
            .child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "proxy-hop-label",
                        (&label, &value),
                        format!("{label}:"),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(self.render_selectable_text_scoped(
                        "proxy-hop-value",
                        (&label, &value),
                        value.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_remove_jump_button(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                LucideIcon::Trash2,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Sm)
            },
        )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut()
                        && index < form.proxy_hops.len()
                    {
                        form.proxy_hops.remove(index);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_jump_auth_tabs(&self, active_tab: SshAuthTab, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (tab, key) in tabs {
            row = row.child(
                segmented_tab(&self.tokens, self.i18n.t(key), tab == active_tab).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut()
                            && let Some(jump_form) = form.jump_server_form.as_mut()
                        {
                            jump_form.auth_tab = tab;
                            form.focused_field = match tab {
                                SshAuthTab::Password => NewConnectionField::JumpPassword,
                                SshAuthTab::SshKey | SshAuthTab::Certificate => {
                                    NewConnectionField::JumpKeyPath
                                }
                                _ => NewConnectionField::JumpHost,
                            };
                            clear_connection_selection(form);
                        }
                        cx.notify();
                    }),
                ),
            );
        }
        form_field(&self.tokens, self.i18n.t("ssh.form.proxy_jump_auth"), row)
    }

    fn add_pending_jump_server(&mut self, cx: &mut Context<Self>) {
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
