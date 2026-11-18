use super::*;

impl QuickCommandsView {
    pub(crate) fn render_quick_command_body(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        contexts: &[QuickCommandTargetContext],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.render_quick_command_toolbar(snapshot, cx));
        if let Some(execution) = snapshot.pending_execution.as_ref() {
            return body
                .child(self.render_quick_command_execution(execution, snapshot, contexts, cx))
                .into_any_element();
        }

        body.when_some(snapshot.category_editor.as_ref(), |body, _| {
            body.child(self.render_quick_command_category_editor(snapshot, cx))
        })
        .when_some(snapshot.command_editor.as_ref(), |body, _| {
            body.child(self.render_quick_command_editor(snapshot, cx))
        })
        .child(self.render_quick_command_rows(snapshot, cx))
        .into_any_element()
    }

    pub(super) fn render_quick_command_toolbar(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .p(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::Search,
                        snapshot.query.clone(),
                        snapshot.focused_input,
                        self.i18n.t("terminal.search_placeholder"),
                        cx,
                    )),
            )
            .child(
                div()
                    .h(px(32.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .cursor_pointer()
                    .text_color(rgb(theme.text_muted))
                    .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            if this.state.read(cx).manager_open() {
                                this.start_quick_command_create(cx);
                                window.focus(&this.focus, cx);
                            } else {
                                this.dispatch(QuickCommandsRequest::OpenManager, Some(window), cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        if snapshot.managing {
                            LucideIcon::Plus
                        } else {
                            LucideIcon::Settings
                        },
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-add-button",
                        "label",
                        self.i18n.t(if snapshot.managing {
                            "terminal.add"
                        } else {
                            "terminal.manage"
                        }),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_quick_command_execution(
        &self,
        execution: &QuickCommandExecutionDraft,
        snapshot: &QuickCommandsRenderSnapshot,
        contexts: &[QuickCommandTargetContext],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let parameter_values = execution
            .command
            .parameters
            .iter()
            .zip(&execution.parameter_values)
            .map(|(parameter, value)| (parameter.name.clone(), value.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let prepared = oxideterm_quick_commands::prepare_quick_command(
            &execution.command,
            contexts,
            &parameter_values,
        );
        let can_run = prepared
            .as_ref()
            .is_ok_and(|prepared| !prepared.targets.is_empty());
        let mut parameters = div().flex().flex_col().gap(px(8.0));
        for (index, parameter) in execution.command.parameters.iter().enumerate() {
            let value = execution
                .parameter_values
                .get(index)
                .cloned()
                .unwrap_or_default();
            let input = if parameter.kind == QuickCommandParameterKind::Secret {
                self.render_quick_command_secret_input(
                    QuickCommandInput::ParameterDefault(index),
                    &value,
                    snapshot.focused_input,
                    parameter.label.clone(),
                    cx,
                )
            } else {
                self.render_quick_command_text_input(
                    QuickCommandInput::ParameterDefault(index),
                    value.to_string(),
                    snapshot.focused_input,
                    parameter.label.clone(),
                    cx,
                )
            };
            let mut field = div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(parameter.label.clone()),
                )
                .child(input);
            if parameter.kind == QuickCommandParameterKind::Choice {
                let mut choices = div().flex().items_center().gap(px(4.0)).flex_wrap();
                for choice in &parameter.choices {
                    let choice_for_click = choice.clone();
                    let selected = choice == value.as_str();
                    choices = choices.child(
                        self.quick_command_text_button(
                            choice.clone(),
                            true,
                            cx.listener(move |this, _event, _window, cx| {
                                this.state.update(cx, |terminal, _cx| {
                                    terminal.set_execution_parameter_value(
                                        index,
                                        choice_for_click.clone(),
                                    )
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .border_color(if selected {
                            rgb(theme.accent)
                        } else {
                            rgba((theme.border << 8) | 0x99)
                        }),
                    );
                }
                field = field.child(choices);
            }
            parameters = parameters.child(field);
        }
        let contains_secret_values = execution
            .command
            .parameters
            .iter()
            .zip(&execution.parameter_values)
            .any(|(parameter, value)| {
                parameter.kind == QuickCommandParameterKind::Secret && !value.is_empty()
            });
        let preview = match &prepared {
            Ok(prepared) => {
                let mut rows = div().flex().flex_col().gap(px(6.0));
                for target in &prepared.targets {
                    let risk = target.risk;
                    rows = rows.child(
                        div()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x99))
                            .p(px(7.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(target.label.clone()),
                                    )
                                    .when_some(risk, |row, risk| {
                                        row.child(status_pill(
                                            &self.tokens,
                                            quick_command_risk_label(&self.i18n, risk)
                                                .to_uppercase(),
                                            StatusPillOptions::new(quick_command_risk_tone(risk))
                                                .compact(),
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_family(self.mono_font.clone())
                                    .text_color(rgb(theme.accent))
                                    .child(if contains_secret_values {
                                        self.i18n.t("terminal.preview_contains_secrets")
                                    } else {
                                        // GPUI owns one frame-local display copy; the runtime
                                        // source remains zeroizing and is cleared on close.
                                        target.command.to_string()
                                    }),
                            ),
                    );
                }
                if !prepared.unavailable_targets.is_empty() {
                    rows =
                        rows.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.warning))
                                .child(self.i18n.t("terminal.unavailable_targets").replace(
                                    "{{targets}}",
                                    &prepared.unavailable_targets.join(", "),
                                )),
                        );
                }
                rows.into_any_element()
            }
            Err(errors) => div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba((theme.error << 8) | 0x99))
                .p(px(7.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(theme.error))
                .child(
                    errors
                        .iter()
                        .map(|error| quick_command_template_error_label(&self.i18n, error))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
                .into_any_element(),
        };
        let form = div()
            .w_full()
            .flex_none()
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text))
                    .child(
                        self.i18n
                            .t("terminal.execution_title")
                            .replace("{{name}}", &execution.command.name),
                    ),
            )
            .when(!execution.command.parameters.is_empty(), |body| {
                body.child(parameters)
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.preview")),
            )
            .child(preview)
            .into_any_element();
        let footer = div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(self.quick_command_text_button(
                self.i18n.t("terminal.cancel"),
                true,
                cx.listener(|this, _event, _window, cx| {
                    this.cancel_terminal_quick_command_execution(cx);
                    cx.stop_propagation();
                }),
            ))
            .child(
                self.quick_command_text_button(
                    self.i18n.t("terminal.run"),
                    can_run,
                    cx.listener(|this, _event, window, cx| {
                        this.dispatch(QuickCommandsRequest::ConfirmExecution, Some(window), cx);
                        cx.stop_propagation();
                    }),
                )
                .bg(if can_run {
                    rgba((theme.accent << 8) | 0x26)
                } else {
                    rgba(0x00000000)
                }),
            )
            .p(px(12.0))
            .into_any_element();
        let scroll = self.state.read(cx).editor_scroll.clone();
        div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(quick_command_editor_frame(
                form,
                footer,
                &scroll,
                &self.tokens,
            ))
            .into_any_element()
    }

    pub(super) fn render_quick_command_rows(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if snapshot.visible_commands.is_empty() {
            return div()
                .h(px(180.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(rgb(theme.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Zap,
                    20.0,
                    rgb(theme.text_muted),
                ))
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "quick-commands-empty",
                    snapshot.query.as_str(),
                    if snapshot.query.trim().is_empty() {
                        self.i18n.t("terminal.empty_category")
                    } else {
                        self.i18n.t("terminal.empty_search")
                    },
                    theme.text_muted,
                    cx,
                ))
                .into_any_element();
        }

        let state = snapshot.list_state.clone();
        let spec = TauriVirtualListSpec::new(
            px(QUICK_COMMAND_LIST_ESTIMATED_HEIGHT),
            QUICK_COMMAND_LIST_OVERSCAN,
        );
        let workspace = cx.entity();
        let visible_commands = snapshot.visible_commands.clone();
        let pinned = snapshot.pinned;
        let managing = snapshot.managing;
        let highlighted_command = snapshot.highlighted_command.clone();
        div()
            .flex_1()
            .min_h(px(0.0))
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    let visible_commands = visible_commands.clone();
                    let highlighted_command = highlighted_command.clone();
                    workspace.update(cx, |this, cx| {
                        this.render_quick_command_list_item(
                            index,
                            &visible_commands,
                            pinned,
                            managing,
                            highlighted_command.as_deref(),
                            cx,
                        )
                    })
                },
            ))
            .into_any_element()
    }

    pub(super) fn render_quick_command_list_item(
        &self,
        index: usize,
        visible_commands: &[QuickCommand],
        pinned: bool,
        managing: bool,
        highlighted_command: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total = visible_commands.len();
        let Some(command) = visible_commands.get(index).cloned() else {
            return div().into_any_element();
        };
        div()
            .px(px(8.0))
            .when(index == 0, |item| item.pt(px(8.0)))
            .pb(px(if index + 1 == total { 8.0 } else { 4.0 }))
            .child(self.render_quick_command_row(
                command,
                pinned,
                managing,
                highlighted_command,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_quick_command_row(
        &self,
        command: QuickCommand,
        pinned: bool,
        managing: bool,
        highlighted_command: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let risk = quick_command_risk_badge(&command.command);
        let command_for_insert = command.command.clone();
        let command_for_run = command.clone();
        let command_for_primary_action = command.clone();
        let command_id_for_move_up = command.id.clone();
        let command_id_for_move_down = command.id.clone();
        let command_id_for_delete = command.id.clone();
        let command_id_for_hover = command.id.clone();
        let keep_open_for_insert = pinned;
        let highlighted = highlighted_command == Some(command.id.as_str());
        let selection_group_id = selection_id("quick-command-row", command.id.as_str());
        div()
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(rgb(theme.text_muted))
            .bg(if highlighted {
                rgba((theme.bg_hover << 8) | 0xb3)
            } else {
                rgba(0x00000000)
            })
            .hover(move |style| {
                style
                    .bg(rgba((theme.bg_hover << 8) | 0xb3))
                    .text_color(rgb(theme.text))
            })
            .on_mouse_move(
                cx.listener(move |this, _event: &gpui::MouseMoveEvent, _window, cx| {
                    // Mouse hover and ArrowUp/ArrowDown share the same active
                    // row state, matching browser menu focus without changing
                    // row-safe selectable click bubbling.
                    if this.state.update(cx, |terminal, _cx| {
                        terminal
                            .set_highlighted_command(command_id_for_hover.clone())
                    }) {
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if managing {
                                this.start_quick_command_edit(
                                    command_for_primary_action.clone(),
                                    cx,
                                );
                            } else {
                                this.insert_quick_command_into_command_bar(
                                    command_for_insert.clone(),
                                    keep_open_for_insert,
                                    window,
                                    cx,
                                );
                            }
                            if managing {
                                window.focus(&this.focus, cx);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.render_row_safe_selectable_display_text_in_group(
                                        selection_group_id,
                                        "quick-command-row-cell",
                                        ("name", command.id.as_str()),
                                        0,
                                        command.name.clone(),
                                        theme.text,
                                        None,
                                        cx,
                                    )),
                            )
                            .when_some(risk, |row, risk| {
                                row.child(
                                    status_pill(
                                        &self.tokens,
                                        quick_command_risk_badge_label(&self.i18n, risk)
                                            .to_uppercase(),
                                        StatusPillOptions::new(quick_command_risk_badge_tone(risk))
                                            .compact()
                                            .strong(),
                                    ),
                                )
                            })
                            .when_some(
                                (!command.availability.host_patterns.is_empty())
                                    .then(|| format!("{}: {}", self.i18n.t(match command.availability.match_field {
                                        QuickCommandMatchField::Host => "terminal.match_host",
                                        QuickCommandMatchField::ConnectionName => "terminal.match_connection",
                                        QuickCommandMatchField::All => "terminal.match_all",
                                    }), command.availability.host_patterns.join(", "))),
                                |row, pattern| {
                                row.child(
                                    status_pill(
                                        &self.tokens,
                                        pattern,
                                        StatusPillOptions::new(StatusTone::Neutral).compact(),
                                    )
                                    .font_family(self.mono_font.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_family(self.mono_font.clone())
                            .text_color(rgba((theme.accent << 8) | 0xd9))
                            .child(self.render_row_safe_selectable_display_text_in_group_with_alpha(
                                selection_group_id,
                                "quick-command-row-cell",
                                ("command", command.id.as_str()),
                                1,
                                command.command.clone(),
                                theme.accent,
                                0xd9 as f32 / 255.0,
                                None,
                                cx,
                            )),
                    )
                    .when_some(command.description.as_ref(), |row, description| {
                        row.child(
                            div()
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgba((theme.text_muted << 8) | 0xcc))
                                .child(self.render_row_safe_selectable_display_text_in_group_with_alpha(
                                    selection_group_id,
                                    "quick-command-row-cell",
                                    ("description", command.id.as_str()),
                                    2,
                                    description.clone(),
                                    theme.text_muted,
                                    0xcc as f32 / 255.0,
                                    None,
                                    cx,
                                )),
                        )
                    }),
            )
            .child(self.quick_command_action_button(
                LucideIcon::Play,
                move |this, _event, window, cx| {
                    this.dispatch(QuickCommandsRequest::Run(command_for_run.clone()), Some(window), cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .when(managing, |row| {
                row.child(self.quick_command_action_button(
                    LucideIcon::ArrowUp,
                    move |this, _event, _window, cx| {
                        this.state.update(cx, |terminal, _cx| {
                            terminal
                                .store
                                .move_command(&command_id_for_move_up, -1)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.quick_command_action_button(
                    LucideIcon::ArrowDown,
                    move |this, _event, _window, cx| {
                        this.state.update(cx, |terminal, _cx| {
                            terminal
                                .store
                                .move_command(&command_id_for_move_down, 1)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.quick_command_action_button(
                    LucideIcon::Trash2,
                    move |this, _event, _window, cx| {
                        this.state.update(cx, |terminal, _cx| {
                            terminal.delete_command(&command_id_for_delete)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ))
            })
            .into_any_element()
    }
}
