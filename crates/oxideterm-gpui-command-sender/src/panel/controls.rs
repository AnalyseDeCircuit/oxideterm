use super::*;
impl ExpandedView {
    pub(super) fn render_terminal_command_sender_controls(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        targets: &[(PaneId, String)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let interval_step = match snapshot.pacing {
            TerminalSenderPacing::Line => TERMINAL_SENDER_INTERVAL_STEP_LINE_MS,
            TerminalSenderPacing::Character => TERMINAL_SENDER_INTERVAL_STEP_CHARACTER_MS,
        };
        let status_label = self.terminal_command_sender_status_label(snapshot);
        let progress_percent = if snapshot.total_units == 0 {
            0.0
        } else {
            (snapshot.completed_units as f32 / snapshot.total_units as f32) * 100.0
        };
        let running = snapshot.status == TerminalCommandSenderStatus::Running;
        let selected_target_count = targets
            .iter()
            .filter(|(id, _)| snapshot.selected_targets.contains(id))
            .count();
        let current_target_label = self.frame.current.clone();
        let status_color = match snapshot.status {
            TerminalCommandSenderStatus::Completed => self.frame.tokens.ui.success,
            TerminalCommandSenderStatus::Failed => self.frame.tokens.ui.error,
            TerminalCommandSenderStatus::Running => self.frame.tokens.ui.accent,
            TerminalCommandSenderStatus::Stopped => self.frame.tokens.ui.warning,
            TerminalCommandSenderStatus::Idle => self.frame.tokens.ui.text_muted,
        };
        let format_controls = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_wrap()
            .child(self.render_terminal_sender_mode_control(snapshot, cx))
            .child(self.render_terminal_sender_pacing_control(snapshot, cx))
            .child(self.render_terminal_sender_stepper(
                self.i18n.t("terminal.sender.interval"),
                format!("{} ms", snapshot.interval_ms),
                54.0,
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_interval(sender_id, -interval_step, cx);
                    });
                },
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_interval(sender_id, interval_step, cx);
                    });
                },
                cx,
            ))
            .child(self.render_terminal_sender_stepper(
                self.i18n.t("terminal.sender.repeat"),
                format!("{}×", snapshot.repeat_count),
                36.0,
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_repeat_count(sender_id, -1, cx);
                    });
                },
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_repeat_count(sender_id, 1, cx);
                    });
                },
                cx,
            ));

        let target_context = (snapshot.target_scope == TerminalCommandSenderTargetScope::Current)
            .then(|| {
                div()
                    .flex_none()
                    .max_w(px(TERMINAL_SENDER_TARGET_CONTEXT_MAX_WIDTH))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.frame.tokens.ui.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        12.0,
                        rgb(self.frame.tokens.ui.text_muted),
                    ))
                    .child(div().truncate().child(current_target_label))
                    .into_any_element()
            });
        let selected_targets = (snapshot.target_scope
            == TerminalCommandSenderTargetScope::Selected)
            .then(|| self.render_terminal_sender_target_list(snapshot, targets, cx));
        let selected_group = (snapshot.target_scope == TerminalCommandSenderTargetScope::Group)
            .then(|| self.render_terminal_sender_group_list(snapshot, cx));
        let target_controls = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .flex_wrap()
            .child(
                div()
                    .flex_none()
                    .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.frame.tokens.ui.text_muted))
                    .child(self.i18n.t("terminal.sender.targets")),
            )
            .child(self.render_terminal_sender_scope_control(snapshot, selected_target_count, cx))
            .when_some(target_context, |row, context| row.child(context))
            .when_some(selected_targets, |row, target_list| {
                row.child(
                    div()
                        .min_w(px(TERMINAL_SENDER_TARGET_LIST_MIN_WIDTH))
                        .flex_1()
                        .child(target_list),
                )
            })
            .when_some(selected_group, |row, group_list| {
                row.child(div().min_w(px(160.0)).flex_1().child(group_list))
            })
            .when(
                !matches!(
                    snapshot.target_scope,
                    TerminalCommandSenderTargetScope::Selected
                        | TerminalCommandSenderTargetScope::Group
                ),
                |row| row.child(div().flex_1().min_w(px(8.0))),
            )
            .when(
                snapshot.status != TerminalCommandSenderStatus::Idle,
                |row| {
                    row.child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(rgb(status_color))
                            .child(status_label),
                    )
                    .child(
                        monospace_datum(
                            &self.frame.tokens,
                            format!(
                                "{}/{} · {} {} · {} {}",
                                snapshot.completed_units,
                                snapshot.total_units,
                                self.i18n.t("terminal.sender.accepted"),
                                snapshot.accepted_writes,
                                self.i18n.t("terminal.sender.skipped"),
                                snapshot.skipped_writes
                            ),
                            Some(self.frame.mono_font.clone()),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Muted).text_size(10.0),
                        )
                        .whitespace_nowrap(),
                    )
                },
            )
            .child(self.terminal_sender_run_button(sender_id, running, cx));

        div()
            .flex_none()
            .border_t_1()
            .border_color(self.frame.divider)
            .when(!self.frame.has_background, |controls| {
                controls.bg(rgb(self.frame.tokens.ui.bg))
            })
            .when(snapshot.total_units > 0, |panel| {
                panel.child(
                    div()
                        .h(px(TERMINAL_SENDER_PROGRESS_HEIGHT))
                        .w_full()
                        .bg(self.frame.divider)
                        .child(
                            div()
                                .h_full()
                                .w(relative((progress_percent / 100.0).clamp(0.0, 1.0)))
                                .bg(rgb(self.frame.tokens.ui.accent)),
                        ),
                )
            })
            .child(
                div()
                    .min_h(px(36.0))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(5.0))
                    .flex()
                    .items_center()
                    .child(format_controls),
            )
            .child(
                div()
                    .min_h(px(36.0))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(5.0))
                    .border_t_1()
                    .border_color(self.frame.divider)
                    .flex()
                    .items_center()
                    .child(target_controls),
            )
            .into_any_element()
    }

    fn render_terminal_sender_segmented_group(&self, items: Vec<AnyElement>) -> AnyElement {
        div()
            .flex_none()
            .h(px(TERMINAL_SENDER_CONTROL_HEIGHT))
            .p(px(2.0))
            .rounded(px(self.frame.tokens.radii.sm))
            .border_1()
            .border_color(self.frame.divider)
            .bg(rgba((self.frame.tokens.ui.bg_sunken << 8) | 0xa6))
            .flex()
            .items_center()
            .gap(px(2.0))
            .children(items)
            .into_any_element()
    }

    fn terminal_sender_segment_item(&self, label: String, active: bool) -> gpui::Div {
        let theme = self.frame.tokens.ui;
        div()
            .h_full()
            .px(px(10.0))
            .rounded(px((self.frame.tokens.radii.sm - 1.0).max(2.0)))
            .flex()
            .items_center()
            .justify_center()
            .whitespace_nowrap()
            .text_size(px(self.frame.tokens.metrics.ui_text_xs))
            .font_weight(if active {
                gpui::FontWeight::MEDIUM
            } else {
                gpui::FontWeight::NORMAL
            })
            .bg(if active {
                rgba((theme.accent << 8) | 0x1e)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .cursor_pointer()
            .when(!active, |item| {
                item.hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
            })
            .child(label)
    }

    fn render_terminal_sender_mode_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let items = [TerminalSenderInputMode::Text, TerminalSenderInputMode::Hex]
            .into_iter()
            .map(|mode| {
                let active = snapshot.input_mode == mode;
                let label = self.i18n.t(match mode {
                    TerminalSenderInputMode::Text => "terminal.sender.text",
                    TerminalSenderInputMode::Hex => "terminal.sender.hex",
                });
                self.terminal_sender_segment_item(label, active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_input_mode(sender_id, mode, cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        self.render_terminal_sender_segmented_group(items)
    }

    fn render_terminal_sender_pacing_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let items = [TerminalSenderPacing::Line, TerminalSenderPacing::Character]
            .into_iter()
            .map(|pacing| {
                let active = snapshot.pacing == pacing;
                let label = self.i18n.t(match pacing {
                    TerminalSenderPacing::Line => "terminal.sender.by_line",
                    TerminalSenderPacing::Character
                        if snapshot.input_mode == TerminalSenderInputMode::Hex =>
                    {
                        "terminal.sender.by_byte"
                    }
                    TerminalSenderPacing::Character => "terminal.sender.by_character",
                });
                self.terminal_sender_segment_item(label, active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_pacing(sender_id, pacing, cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        self.render_terminal_sender_segmented_group(items)
    }

    fn render_terminal_sender_scope_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        selected_target_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let items = [
            TerminalCommandSenderTargetScope::Current,
            TerminalCommandSenderTargetScope::All,
            TerminalCommandSenderTargetScope::Selected,
            TerminalCommandSenderTargetScope::Group,
        ]
        .into_iter()
        .map(|scope| {
            let active = snapshot.target_scope == scope;
            let label = match scope {
                TerminalCommandSenderTargetScope::Current => self.i18n.t("terminal.sender.current"),
                TerminalCommandSenderTargetScope::All => self.i18n.t("terminal.sender.all"),
                TerminalCommandSenderTargetScope::Selected => format!(
                    "{} ({selected_target_count})",
                    self.i18n.t("terminal.sender.selected")
                ),
                TerminalCommandSenderTargetScope::Group => format!(
                    "{} ({selected_target_count})",
                    self.i18n.t("terminal.sender.group")
                ),
            };
            self.terminal_sender_segment_item(label, active)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.set_target_scope(sender_id, scope, cx);
                        });
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        })
        .collect();
        self.render_terminal_sender_segmented_group(items)
    }

    fn render_terminal_sender_target_list(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        targets: &[(PaneId, String)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let mut target_row = div().flex_none().flex().items_center().gap(px(4.0));
        for (id, label) in targets {
            let pane_id = *id;
            let selected = snapshot.selected_targets.contains(&pane_id);
            let options = ActionChipOptions::new()
                .active(selected)
                .height(24.0)
                .radius(ButtonRadius::Sm)
                .idle_text_tone(ActionChipTextTone::Muted);
            target_row = target_row.child(
                action_chip(
                    &self.frame.tokens,
                    label.clone(),
                    Some(Self::render_lucide_icon(
                        if selected {
                            LucideIcon::CheckSquare
                        } else {
                            LucideIcon::Square
                        },
                        12.0,
                        action_chip_foreground(&self.frame.tokens, options),
                    )),
                    options,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.toggle_selected_target(sender_id, pane_id, cx);
                        });
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        // The inner row retains content-sized target chips while the outer
        // viewport owns horizontal overflow at narrow terminal widths.
        div()
            .w_full()
            .overflow_x_scrollbar()
            .child(target_row)
            .into_any_element()
    }

    fn render_terminal_sender_group_list(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let groups = &self.frame.groups;
        if groups.is_empty() {
            return div()
                .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.frame.tokens.ui.text_muted))
                .child(self.i18n.t("terminal.sender.no_groups"))
                .into_any_element();
        }

        let mut group_row = div().flex_none().flex().items_center().gap(px(4.0));
        for group in groups {
            let group_id = group.0;
            let selected = snapshot.selected_group_id == Some(group_id);
            let options = ActionChipOptions::new()
                .active(selected)
                .height(24.0)
                .radius(ButtonRadius::Sm)
                .idle_text_tone(ActionChipTextTone::Muted);
            group_row = group_row.child(
                action_chip(
                    &self.frame.tokens,
                    group.1.clone(),
                    Some(Self::render_lucide_icon(
                        if selected {
                            LucideIcon::CheckSquare
                        } else {
                            LucideIcon::Square
                        },
                        12.0,
                        action_chip_foreground(&self.frame.tokens, options),
                    )),
                    options,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        (this.dispatch)(SenderAction::Group(sender_id, group_id), _window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(div().overflow_x_scrollbar().child(group_row))
            .child(
                div()
                    .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.frame.tokens.ui.text_muted))
                    .child(self.i18n.t("terminal.sender.group_hint")),
            )
            .into_any_element()
    }

    fn render_terminal_sender_stepper(
        &self,
        label: String,
        value: String,
        min_value_width: f32,
        decrement: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        increment: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.frame.tokens.ui;
        let divider = self.frame.divider;
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .h(px(TERMINAL_SENDER_CONTROL_HEIGHT))
                    .rounded(px(self.frame.tokens.radii.sm))
                    .border_1()
                    .border_color(divider)
                    .bg(rgba((theme.bg_sunken << 8) | 0xa6))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(px(22.0))
                            .border_r_1()
                            .border_color(divider)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_secondary))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child("−")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    decrement(this, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .h_full()
                            .min_w(px(min_value_width))
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                            .font_family(self.frame.mono_font.clone())
                            .text_color(rgb(theme.text))
                            .child(value),
                    )
                    .child(
                        div()
                            .h_full()
                            .w(px(22.0))
                            .border_l_1()
                            .border_color(divider)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_secondary))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child("+")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    increment(this, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn terminal_sender_run_button(
        &self,
        sender_id: TerminalCommandSenderId,
        running: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.frame.tokens.ui;
        let label = if running {
            self.i18n.t("terminal.sender.stop")
        } else {
            self.i18n.t("terminal.sender.start")
        };
        let tone_hex = if running { theme.error } else { theme.accent };
        let mut options = oxideterm_gpui_ui::button::ToolbarButtonOptions::compact_text_min_width(
            oxideterm_gpui_ui::button::ButtonVariant::Outline,
            oxideterm_gpui_ui::button::ButtonRadius::Sm,
            TERMINAL_SENDER_CONTROL_HEIGHT,
            64.0,
            12.0,
            self.frame.tokens.metrics.ui_text_xs,
        );
        options.background = Some(rgba((tone_hex << 8) | 0x1a));
        options.border = Some(rgba((tone_hex << 8) | 0x66));
        options.text_color = Some(rgb(tone_hex));
        options.hover_background = Some(rgba((tone_hex << 8) | 0x28));
        options.hover_border = Some(rgb(tone_hex));
        options.hover_text_color = Some(rgb(tone_hex));
        oxideterm_gpui_ui::button::toolbar_button(&self.frame.tokens, label, None, options)
            .debug_selector(|| "sender-run-action".to_owned())
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if running {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.stop(sender_id, cx);
                        });
                    } else {
                        (this.dispatch)(SenderAction::Start(sender_id), _window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn terminal_command_sender_status_label(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
    ) -> String {
        if let Some(failure) = snapshot.failure {
            return self.i18n.t(match failure {
                TerminalCommandSenderFailure::EmptyInput => "terminal.sender.error_empty",
                TerminalCommandSenderFailure::InvalidHex => "terminal.sender.error_hex",
                TerminalCommandSenderFailure::NoTargets => "terminal.sender.error_targets",
                TerminalCommandSenderFailure::TargetBusy => "terminal.sender.error_busy",
            });
        }
        self.i18n.t(match snapshot.status {
            TerminalCommandSenderStatus::Idle => "terminal.sender.idle",
            TerminalCommandSenderStatus::Running => "terminal.sender.running",
            TerminalCommandSenderStatus::Completed => "terminal.sender.completed",
            TerminalCommandSenderStatus::Stopped => "terminal.sender.stopped",
            TerminalCommandSenderStatus::Failed => "terminal.sender.failed",
        })
    }
}
