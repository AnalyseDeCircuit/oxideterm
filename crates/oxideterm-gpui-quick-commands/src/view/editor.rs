use super::*;

impl QuickCommandsView {
    pub(super) fn render_quick_command_category_editor(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = snapshot.category_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = quick_command_category_draft_can_save(draft);
        let parsed_color = oxideterm_quick_commands::parse_icon_color(&draft.icon_color);
        let invalid_color = !draft.icon_color.trim().is_empty() && parsed_color.is_none();
        let mut icon_options = div().flex().items_center().gap(px(4.0));
        for icon in [
            QuickCommandIcon::Terminal,
            QuickCommandIcon::Server,
            QuickCommandIcon::Folder,
            QuickCommandIcon::Docker,
            QuickCommandIcon::Zap,
        ] {
            let active = draft.icon == icon;
            icon_options = icon_options.child(
                div()
                    .h(px(30.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.state
                                .update(cx, |terminal, _cx| terminal.set_category_icon(icon));
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        quick_command_lucide_icon(icon),
                        13.0,
                        parsed_color.map(rgb).unwrap_or_else(|| {
                            if active {
                                rgb(theme.accent)
                            } else {
                                rgb(theme.text_muted)
                            }
                        }),
                    ))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-icon-option",
                        quick_command_icon_source_id(icon),
                        self.i18n.t(&quick_command_icon_label_key(icon)),
                        if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                        cx,
                    )),
            );
        }
        div()
            .bg(rgba((theme.bg << 8) | 0x59))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(snapshot.managing, |editor| {
                editor.h_full().min_h(px(0.0)).p(px(16.0))
            })
            .when(!snapshot.managing, |editor| {
                editor
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(8.0))
            })
            .overflow_y_scrollbar()
            .when(snapshot.managing, |editor| {
                editor.child(
                    div()
                        .mb(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t(if draft.id.is_some() {
                            "terminal.quick_commands.edit_group"
                        } else {
                            "terminal.quick_commands.add_group"
                        })),
                )
            })
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CategoryName,
                            draft.name.clone(),
                            snapshot.focused_input,
                            self.i18n
                                .t("terminal.quick_commands.group_name_placeholder"),
                            cx,
                        ),
                    )
                    .child(icon_options)
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .child(self.i18n.t("terminal.quick_commands.icon_color")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .size(px(28.0))
                                    .flex_none()
                                    .rounded(px(self.tokens.radii.md))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .bg(rgb(parsed_color.unwrap_or(theme.text_muted))),
                            )
                            .child(div().flex_1().min_w_0().child(
                                self.render_quick_command_text_input(
                                    QuickCommandInput::CategoryColor,
                                    draft.icon_color.clone(),
                                    snapshot.focused_input,
                                    self.i18n.t("terminal.quick_commands.icon_color_hint"),
                                    cx,
                                ),
                            ))
                            .child(self.quick_command_text_button(
                                self.i18n.t("terminal.quick_commands.icon_color_reset"),
                                !draft.icon_color.is_empty(),
                                cx.listener(|this, _, _, cx| {
                                    this.dispatch(QuickCommandsRequest::ClearIme, None, cx);
                                    this.state.update(cx, |terminal, _| {
                                        if let Some(draft) = terminal.store.category_editor.as_mut()
                                        {
                                            draft.icon_color.clear();
                                        }
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )),
                    )
                    .when(invalid_color, |column| {
                        column.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(theme.error))
                                .child(self.i18n.t("terminal.quick_commands.icon_color_invalid")),
                        )
                    }),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save_group",
                |this, cx| this.save_quick_command_category_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_quick_command_editor(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = snapshot.command_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = quick_command_editor_can_save(draft);
        let mut categories = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for category in &snapshot.categories {
            let category_id = category.id.clone();
            let active = draft.category == category.id;
            categories = categories.child(
                div()
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.state.update(cx, |terminal, _cx| {
                                terminal.set_command_category(category_id.clone())
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-editor-category",
                        category.id.as_str(),
                        category.name.clone(),
                        if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                        cx,
                    )),
            );
        }

        let mut protocols = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for (protocol, label_key) in [
            (
                QuickCommandTargetProtocol::Local,
                "terminal.quick_commands.protocol_local",
            ),
            (
                QuickCommandTargetProtocol::Ssh,
                "terminal.quick_commands.protocol_ssh",
            ),
            (
                QuickCommandTargetProtocol::Mosh,
                "terminal.quick_commands.protocol_mosh",
            ),
            (
                QuickCommandTargetProtocol::Telnet,
                "terminal.quick_commands.protocol_telnet",
            ),
            (
                QuickCommandTargetProtocol::Serial,
                "terminal.quick_commands.protocol_serial",
            ),
            (
                QuickCommandTargetProtocol::Tmux,
                "terminal.quick_commands.protocol_tmux",
            ),
        ] {
            let active = draft.protocols.contains(&protocol);
            protocols = protocols.child(
                self.quick_command_text_button(
                    self.i18n.t(label_key),
                    true,
                    cx.listener(move |this, _event, _window, cx| {
                        this.state.update(cx, |terminal, _cx| {
                            terminal.toggle_command_protocol(protocol)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .border_color(if active {
                    rgb(theme.accent)
                } else {
                    rgba((theme.border << 8) | 0x99)
                })
                .bg(if active {
                    rgba((theme.accent << 8) | 0x1a)
                } else {
                    rgba(0x00000000)
                }),
            );
        }
        let mut match_fields = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for (field, label) in [
            (
                QuickCommandMatchField::Host,
                "terminal.quick_commands.match_host",
            ),
            (
                QuickCommandMatchField::ConnectionName,
                "terminal.quick_commands.match_connection",
            ),
            (
                QuickCommandMatchField::All,
                "terminal.quick_commands.match_all",
            ),
        ] {
            let active = draft.match_field == field;
            match_fields = match_fields.child(
                self.quick_command_text_button(
                    self.i18n.t(label),
                    true,
                    cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |terminal, _| {
                            if let Some(draft) = terminal.store.command_editor.as_mut() {
                                draft.match_field = field;
                            }
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .border_color(if active {
                    rgb(theme.accent)
                } else {
                    rgba((theme.border << 8) | 0x99)
                })
                .bg(if active {
                    rgba((theme.accent << 8) | 0x1a)
                } else {
                    rgba(0x00000000)
                }),
            );
        }
        let parameters = self.render_quick_command_parameter_editor(draft, snapshot, cx);
        let confirmation_always = draft.confirmation == QuickCommandConfirmationPolicy::Always;
        let scroll = self.state.read(cx).editor_scroll.clone();
        let padding = if snapshot.managing { 16.0 } else { 8.0 };

        let form = div()
            .w_full()
            .min_w_0()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(padding))
            .when(snapshot.managing, |editor| {
                editor.child(
                    div()
                        .mb(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t(if draft.id.is_some() {
                            "terminal.quick_commands.edit_command"
                        } else {
                            "terminal.quick_commands.add_command"
                        })),
                )
            })
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap(px(8.0))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandName,
                        draft.name.clone(),
                        snapshot.focused_input,
                        self.i18n.t("terminal.quick_commands.name_placeholder"),
                        cx,
                    ))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandText,
                        draft.command.clone(),
                        snapshot.focused_input,
                        self.i18n.t("terminal.quick_commands.command_placeholder"),
                        cx,
                    ))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("terminal.quick_commands.template_hint")),
                    )
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandDescription,
                            draft.description.clone(),
                            snapshot.focused_input,
                            self.i18n
                                .t("terminal.quick_commands.description_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("terminal.quick_commands.match_field")),
                            )
                            .child(match_fields)
                            .child(
                                self.render_quick_command_text_input(
                                    QuickCommandInput::CommandHostPattern,
                                    draft.host_patterns.clone(),
                                    snapshot.focused_input,
                                    self.i18n
                                        .t("terminal.quick_commands.host_pattern_placeholder"),
                                    cx,
                                ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("terminal.quick_commands.match_hint")),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .max_h(px(QUICK_COMMAND_CATEGORY_PICKER_MAX_HEIGHT))
                            .overflow_y_scrollbar()
                            .child(categories),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("terminal.quick_commands.protocols")),
                            )
                            .child(protocols),
                    )
                    .child(parameters)
                    .child(
                        self.quick_command_text_button(
                            self.i18n.t(if confirmation_always {
                                "terminal.quick_commands.confirmation_always"
                            } else {
                                "terminal.quick_commands.confirmation_inherit"
                            }),
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.state.update(cx, |terminal, _cx| {
                                    terminal.toggle_command_confirmation()
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .border_color(if confirmation_always {
                            rgb(theme.warning)
                        } else {
                            rgba((theme.border << 8) | 0x99)
                        }),
                    ),
            );

        quick_command_editor_frame(
            form.into_any_element(),
            div()
                .p(px(padding))
                .child(self.render_quick_editor_buttons(
                    can_save,
                    "terminal.quick_commands.save",
                    |this, cx| this.save_quick_command_editor(cx),
                    cx,
                ))
                .into_any_element(),
            &scroll,
            &self.tokens,
        )
        .when(!snapshot.managing, |editor| editor.max_h(px(420.0)))
        .into_any_element()
    }

    pub(super) fn render_quick_command_parameter_editor(
        &self,
        draft: &QuickCommandEditorDraft,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut rows = div().flex().flex_col().gap(px(6.0));
        for (index, parameter) in draft.parameters.iter().enumerate() {
            let kind = parameter.kind;
            let required = parameter.required;
            rows = rows.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(div().flex_1().child(self.render_quick_command_text_input(
                                QuickCommandInput::ParameterName(index),
                                parameter.name.clone(),
                                snapshot.focused_input,
                                self.i18n.t("terminal.quick_commands.parameter_name"),
                                cx,
                            )))
                            .child(div().flex_1().child(self.render_quick_command_text_input(
                                QuickCommandInput::ParameterLabel(index),
                                parameter.label.clone(),
                                snapshot.focused_input,
                                self.i18n.t("terminal.quick_commands.parameter_label"),
                                cx,
                            ))),
                    )
                    .when(kind != QuickCommandParameterKind::Secret, |row| {
                        row.child(self.render_quick_command_text_input(
                            QuickCommandInput::ParameterDefault(index),
                            parameter.default_value.clone(),
                            snapshot.focused_input,
                            self.i18n.t("terminal.quick_commands.parameter_default"),
                            cx,
                        ))
                    })
                    .when(kind == QuickCommandParameterKind::Choice, |row| {
                        row.child(self.render_quick_command_text_input(
                            QuickCommandInput::ParameterChoices(index),
                            parameter.choices.clone(),
                            snapshot.focused_input,
                            self.i18n.t("terminal.quick_commands.parameter_choices"),
                            cx,
                        ))
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(self.quick_command_text_button(
                                self.i18n.t(match kind {
                                    QuickCommandParameterKind::Text => {
                                        "terminal.quick_commands.parameter_text"
                                    }
                                    QuickCommandParameterKind::Choice => {
                                        "terminal.quick_commands.parameter_choice"
                                    }
                                    QuickCommandParameterKind::Secret => {
                                        "terminal.quick_commands.parameter_secret"
                                    }
                                }),
                                true,
                                cx.listener(move |this, _event, _window, cx| {
                                    let next = match kind {
                                        QuickCommandParameterKind::Text => {
                                            QuickCommandParameterKind::Choice
                                        }
                                        QuickCommandParameterKind::Choice => {
                                            QuickCommandParameterKind::Secret
                                        }
                                        QuickCommandParameterKind::Secret => {
                                            QuickCommandParameterKind::Text
                                        }
                                    };
                                    this.state.update(cx, |terminal, _cx| {
                                        terminal.set_command_parameter_kind(index, next)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.quick_command_text_button(
                                self.i18n.t(if required {
                                    "terminal.quick_commands.parameter_required"
                                } else {
                                    "terminal.quick_commands.parameter_optional"
                                }),
                                true,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.state.update(cx, |terminal, _cx| {
                                        terminal.toggle_command_parameter_required(index)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.quick_command_icon_button(
                                LucideIcon::Trash2,
                                move |this, _event, _window, cx| {
                                    this.state.update(cx, |terminal, _cx| {
                                        terminal.remove_command_parameter(index)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("terminal.quick_commands.parameters")),
                    )
                    .child(self.quick_command_text_button(
                        self.i18n.t("terminal.quick_commands.add_parameter"),
                        true,
                        cx.listener(|this, _event, _window, cx| {
                            this.state
                                .update(cx, |terminal, _cx| terminal.add_command_parameter());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .child(rows)
            .into_any_element()
    }

    pub(super) fn render_quick_editor_buttons(
        &self,
        can_save: bool,
        save_key: &'static str,
        save: fn(&mut QuickCommandsView, &mut Context<QuickCommandsView>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(self.quick_command_text_button(
                self.i18n.t("terminal.quick_commands.cancel"),
                true,
                cx.listener(|this, _event, _window, cx| {
                    this.state
                        .update(cx, |terminal, _cx| terminal.cancel_editor());
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(
                self.quick_command_text_button(
                    self.i18n.t(save_key),
                    can_save,
                    cx.listener(move |this, _event, _window, cx| {
                        save(this, cx);
                        cx.stop_propagation();
                    }),
                )
                .bg(if can_save {
                    rgba((theme.accent << 8) | 0x26)
                } else {
                    rgba(0x00000000)
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_quick_command_text_input(
        &self,
        input: QuickCommandInput,
        value: String,
        focused_input: Option<QuickCommandInput>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if input == QuickCommandInput::CommandText {
            return self.render_quick_command_multiline_input(input, focused_input, cx);
        }
        self.render_quick_command_input(input, &value, focused_input, placeholder, false, cx)
    }

    pub(super) fn render_quick_command_multiline_input(
        &self,
        input: QuickCommandInput,
        focused_input: Option<QuickCommandInput>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = self.state.read(cx).command_input.clone();
        let focus = editor
            .as_ref()
            .map(|editor| editor.read(cx).focus_handle(cx));
        div()
            .id("quick-command-text-editor")
            .w_full()
            .min_w_0()
            .h(px(QUICK_COMMAND_TEXTAREA_MIN_HEIGHT))
            .flex_none()
            .p(px(QUICK_COMMAND_TEXTAREA_VERTICAL_PADDING))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(if focused_input == Some(input) {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x80))
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, _, window, cx| {
                this.state.update(cx, |terminal, _| {
                    terminal.set_focused_input(input);
                });
                this.dispatch(QuickCommandsRequest::ClearIme, None, cx);
                if let Some(focus) = focus.as_ref() {
                    window.focus(focus, cx);
                }
                cx.notify();
            }))
            .children(editor)
            .into_any_element()
    }

    pub(super) fn render_quick_command_secret_input(
        &self,
        input: QuickCommandInput,
        value: &Zeroizing<String>,
        focused_input: Option<QuickCommandInput>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_quick_command_input(input, value, focused_input, placeholder, true, cx)
    }

    pub(super) fn render_quick_command_input(
        &self,
        input: QuickCommandInput,
        value: &str,
        focused_input: Option<QuickCommandInput>,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = focused_input == Some(input);
        let ime = (self.bindings.ime)(input, cx);
        let viewport = self.state.read(cx).input_viewport(input);
        let anchor = self.bindings.anchor.clone();
        let control = text_input_with_viewport(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: self.caret_visible,
                secret,
                selected_all: false,
                selected_range: ime.selection,
                marked_text: ime.marked.as_ref().map(|text| text.as_str()),
            },
            &viewport,
            ime.active_offset,
        )
        .h(px(32.0))
        .when(quick_command_input_uses_monospace(input), |field| {
            field.font_family(self.mono_font.clone())
        })
        .cursor(gpui::CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.state
                    .update(cx, |state, _| state.set_focused_input(input));
                this.dispatch(
                    QuickCommandsRequest::FocusInput(input, event.clone()),
                    Some(window),
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.dispatch(
                    QuickCommandsRequest::DragInput(event.clone()),
                    Some(window),
                    cx,
                );
            }),
        );
        oxideterm_gpui_ui::text_input::text_input_anchor_probe(
            ime.anchor_id,
            control,
            move |data, _, _| anchor(data),
        )
        .into_any_element()
    }

    pub(super) fn initialize_quick_command_text_editor(&mut self, cx: &mut Context<Self>) {
        let value = self
            .state
            .read(cx)
            .store
            .command_editor
            .as_ref()
            .map(|draft| draft.command.clone())
            .unwrap_or_default();
        let tokens = self.tokens;
        let font_family = self.mono_font.clone().to_string();
        let placeholder = self.i18n.t("terminal.quick_commands.command_placeholder");
        let labels = EditorContextMenuLabels {
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        };
        let workspace = cx.weak_entity();
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(value, &tokens, cx);
            editor.set_settings(
                EditorSettings {
                    soft_wrap: false,
                    indentation_markers: false,
                    highlight_special_chars: false,
                    placeholder: Some(placeholder),
                    ..EditorSettings::default()
                },
                cx,
            );
            editor.set_border_visible(false);
            editor.apply_runtime_settings(
                &tokens,
                font_family,
                tokens.metrics.ui_text_sm,
                QUICK_COMMAND_TEXTAREA_LINE_HEIGHT / tokens.metrics.ui_text_sm,
                false,
                true,
                cx,
            );
            editor.set_context_menu_labels(labels);
            editor.set_on_save(Box::new(move |_, _, cx| {
                let workspace = workspace.clone();
                let editor_id = cx.entity_id();
                // Save after the editor update so the workspace can read its current buffer.
                cx.defer(move |cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        if this
                            .state
                            .read(cx)
                            .command_input
                            .as_ref()
                            .is_some_and(|editor| editor.entity_id() == editor_id)
                        {
                            this.save_quick_command_editor(cx);
                        }
                    });
                });
                Ok(())
            }));
            editor
        });
        let mut version = editor.read(cx).buffer().version();
        let subscription = cx.observe(&editor, move |this, editor, cx| {
            let buffer = editor.read(cx).buffer();
            if buffer.version() == version {
                return;
            }
            version = buffer.version();
            let text = buffer.text();
            this.state.update(cx, |terminal, _| {
                if terminal.command_input.as_ref() == Some(&editor)
                    && let Some(draft) = terminal.store.command_editor.as_mut()
                {
                    draft.command = text;
                }
            });
            cx.notify();
        });
        self.state.update(cx, |terminal, _| {
            terminal.command_input = Some(editor);
            terminal.command_input_subscription = Some(subscription);
            terminal.editor_scroll = gpui::ScrollHandle::new();
        });
    }

    pub(super) fn start_quick_command_create(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |terminal, _cx| terminal.start_command_create());
        self.initialize_quick_command_text_editor(cx);
        cx.notify();
    }

    pub(super) fn start_quick_command_edit(
        &mut self,
        command: QuickCommand,
        cx: &mut Context<Self>,
    ) {
        self.state
            .update(cx, |terminal, _cx| terminal.start_command_edit(command));
        self.initialize_quick_command_text_editor(cx);
        cx.notify();
    }

    pub(super) fn start_quick_command_category_create(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |terminal, _cx| terminal.start_category_create());
        cx.notify();
    }

    pub(super) fn start_quick_command_category_edit(
        &mut self,
        category: QuickCommandCategory,
        cx: &mut Context<Self>,
    ) {
        self.state
            .update(cx, |terminal, _cx| terminal.start_category_edit(category));
        cx.notify();
    }

    pub fn save_quick_command_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.state.read(cx).command_input.clone() {
            let text = editor.read(cx).buffer().text();
            self.state.update(cx, |terminal, _| {
                if let Some(draft) = terminal.store.command_editor.as_mut() {
                    draft.command = text;
                }
            });
        }
        if self
            .state
            .update(cx, |terminal, _cx| terminal.save_command_editor())
        {
            cx.notify();
        }
    }

    pub(super) fn save_quick_command_category_editor(&mut self, cx: &mut Context<Self>) {
        if self
            .state
            .update(cx, |terminal, _cx| terminal.save_category_editor())
        {
            cx.notify();
        }
    }
}
