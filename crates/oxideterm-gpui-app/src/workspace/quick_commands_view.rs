pub(super) fn quick_command_lucide_icon(icon: QuickCommandIcon) -> LucideIcon {
    match icon {
        QuickCommandIcon::Server => LucideIcon::Server,
        QuickCommandIcon::Folder => LucideIcon::Folder,
        QuickCommandIcon::Docker => LucideIcon::Server,
        QuickCommandIcon::Zap => LucideIcon::Zap,
        QuickCommandIcon::Terminal => LucideIcon::Monitor,
    }
}

pub(super) fn quick_command_icon_label_key(icon: QuickCommandIcon) -> String {
    format!("terminal.quick_commands.icon_{}", icon.as_source_id())
}

impl WorkspaceApp {
    pub(super) fn handle_quick_commands_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.quick_commands.focused_input else {
            return;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        match key {
            "escape" => {
                self.quick_commands.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
            }
            "enter" if input == QuickCommandInput::CategoryName => {
                self.save_quick_command_category_editor(cx);
            }
            "enter"
                if matches!(
                    input,
                    QuickCommandInput::CommandName
                        | QuickCommandInput::CommandText
                        | QuickCommandInput::CommandDescription
                        | QuickCommandInput::CommandHostPattern
                ) =>
            {
                self.save_quick_command_editor(cx);
            }
            "backspace" if !modifiers.platform && !modifiers.control => {
                self.quick_command_input_value_mut(input).pop();
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn quick_command_input_value(&self, input: QuickCommandInput) -> Option<String> {
        match input {
            QuickCommandInput::Search => Some(self.quick_commands.query.clone()),
            QuickCommandInput::CommandName => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.name.clone()),
            QuickCommandInput::CommandText => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.command.clone()),
            QuickCommandInput::CommandDescription => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.description.clone()),
            QuickCommandInput::CommandHostPattern => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.host_pattern.clone()),
            QuickCommandInput::CategoryName => self
                .quick_commands
                .category_editor
                .as_ref()
                .map(|draft| draft.name.clone()),
        }
    }

    pub(super) fn quick_command_input_value_mut(
        &mut self,
        input: QuickCommandInput,
    ) -> &mut String {
        match input {
            QuickCommandInput::Search => &mut self.quick_commands.query,
            QuickCommandInput::CommandName => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .name
            }
            QuickCommandInput::CommandText => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .command
            }
            QuickCommandInput::CommandDescription => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .description
            }
            QuickCommandInput::CommandHostPattern => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .host_pattern
            }
            QuickCommandInput::CategoryName => {
                &mut self
                    .quick_commands
                    .category_editor
                    .as_mut()
                    .expect("quick command category editor is open")
                    .name
            }
        }
    }

    pub(super) fn render_quick_commands_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active_label = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_default();
        let visible_commands = self
            .quick_commands
            .visible_commands_for_targets(&[active_label]);
        let mut popover = div()
            .absolute()
            .bottom(px(56.0))
            .right(px(12.0))
            .w(px(860.0))
            .max_w(px(860.0))
            .max_h(px(520.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_elevated << 8) | 0xf2))
            .shadow_lg()
            .flex()
            .text_size(px(12.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                cx.stop_propagation();
            });

        let sidebar = self.render_quick_command_category_sidebar(cx);
        let body = self.render_quick_command_body(visible_commands, cx);
        popover = popover.child(sidebar).child(body);
        popover.into_any_element()
    }

    fn render_quick_command_category_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut sidebar = div()
            .w(px(160.0))
            .flex_none()
            .overflow_hidden()
            .rounded_l(px(self.tokens.radii.lg))
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x73))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .mb(px(2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("terminal.quick_commands.title").to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                quick_command_icon_button(&self.tokens, LucideIcon::Plus)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.start_quick_command_category_create(cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                quick_command_icon_button(&self.tokens, LucideIcon::X)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.terminal_quick_commands_open = false;
                                            this.terminal_quick_command_pending = None;
                                            this.quick_commands.focused_input = None;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
            );

        for category in &self.quick_commands.categories {
            let category_id = category.id.clone();
            let active = category.id == self.quick_commands.active_category;
            let count = self
                .quick_commands
                .commands
                .iter()
                .filter(|command| command.category == category.id)
                .count();
            let can_delete = !default_quick_command_categories()
                .iter()
                .any(|default| default.id == category.id)
                && count == 0;
            sidebar = sidebar.child(
                div()
                    .group("quick-command-category")
                    .rounded(px(self.tokens.radii.md))
                    .px(px(8.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1f)
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener({
                                    let category_id = category_id.clone();
                                    move |this, _event, _window, cx| {
                                        this.quick_commands.active_category = category_id.clone();
                                        this.quick_commands.command_editor = None;
                                        this.quick_commands.category_editor = None;
                                        this.quick_commands.focused_input = None;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                quick_command_lucide_icon(category.icon),
                                14.0,
                                if active {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                            ))
                            .child(div().flex_1().truncate().child(category.name.clone()))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.md))
                                    .bg(rgb(theme.bg_panel))
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(count.to_string()),
                            ),
                    )
                    .child(
                        quick_command_mini_button(&self.tokens, LucideIcon::Pencil).on_mouse_down(
                            MouseButton::Left,
                            cx.listener({
                                let category = category.clone();
                                move |this, _event, _window, cx| {
                                    this.start_quick_command_category_edit(category.clone(), cx);
                                    cx.stop_propagation();
                                }
                            }),
                        ),
                    )
                    .when(can_delete, |row| {
                        row.child(
                            quick_command_mini_button(&self.tokens, LucideIcon::Trash2)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener({
                                        let category_id = category_id.clone();
                                        move |this, _event, _window, cx| {
                                            this.quick_commands.delete_category(&category_id);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }
                                    }),
                                ),
                        )
                    }),
            );
        }

        sidebar
            .child(div().flex_1())
            .when_some(
                self.quick_commands.last_persist_error.as_ref(),
                |sidebar, error| {
                    sidebar.child(
                        div()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba(0xef444480))
                            .bg(rgba(0xef44441a))
                            .p(px(6.0))
                            .text_size(px(10.0))
                            .text_color(rgba(0xfca5a5ff))
                            .child(error.clone()),
                    )
                },
            )
            .into_any_element()
    }

    fn render_quick_command_body(
        &self,
        visible_commands: Vec<QuickCommand>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded_r(px(self.tokens.radii.lg))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(8.0))
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::Search,
                            self.quick_commands.query.clone(),
                            self.i18n.t("terminal.quick_commands.search_placeholder"),
                            cx,
                        ),
                    ))
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
                            .hover(move |style| {
                                style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_quick_command_create(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::Plus,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .child(self.i18n.t("terminal.quick_commands.add")),
                    ),
            )
            .when_some(self.quick_commands.category_editor.as_ref(), |body, _| {
                body.child(self.render_quick_command_category_editor(cx))
            })
            .when_some(self.quick_commands.command_editor.as_ref(), |body, _| {
                body.child(self.render_quick_command_editor(cx))
            })
            .child(self.render_quick_command_rows(visible_commands, cx))
            .into_any_element()
    }

    fn render_quick_command_rows(
        &self,
        visible_commands: Vec<QuickCommand>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if visible_commands.is_empty() {
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
                .child(if self.quick_commands.query.trim().is_empty() {
                    self.i18n.t("terminal.quick_commands.empty_category")
                } else {
                    self.i18n.t("terminal.quick_commands.empty_search")
                })
                .into_any_element();
        }

        let mut list = div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0));
        for command in visible_commands {
            list = list.child(self.render_quick_command_row(command, cx));
        }
        list.into_any_element()
    }

    fn render_quick_command_row(
        &self,
        command: QuickCommand,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let risk = classify_command_risk(&command.command);
        let command_for_insert = command.command.clone();
        let command_for_run = command.command.clone();
        let command_for_edit = command.clone();
        let command_id = command.id.clone();
        div()
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(rgb(theme.text_muted))
            .hover(move |style| {
                style
                    .bg(rgba((theme.bg_hover << 8) | 0xb3))
                    .text_color(rgb(theme.text))
            })
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
                            this.terminal_command_bar_draft = command_for_insert.clone();
                            this.terminal_command_bar_focused = true;
                            this.terminal_quick_commands_open = false;
                            this.quick_commands.focused_input = None;
                            window.focus(&this.focus_handle);
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
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(command.name.clone()),
                            )
                            .when_some(risk, |row, risk: &'static str| {
                                row.child(
                                    div()
                                        .rounded(px(self.tokens.radii.md))
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .text_size(px(10.0))
                                        .text_color(if risk == "high" {
                                            rgba(0xfca5a5ff)
                                        } else {
                                            rgba(0xfcd34dff)
                                        })
                                        .bg(if risk == "high" {
                                            rgba(0xef444426)
                                        } else {
                                            rgba(0xf59e0b26)
                                        })
                                        .child(risk.to_uppercase()),
                                )
                            })
                            .when_some(command.host_pattern.as_ref(), |row, pattern| {
                                row.child(
                                    div()
                                        .rounded(px(self.tokens.radii.md))
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme.text_muted))
                                        .bg(rgb(theme.bg_panel))
                                        .child(pattern.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(rgba((theme.accent << 8) | 0xd9))
                            .child(command.command.clone()),
                    )
                    .when_some(command.description.as_ref(), |row, description| {
                        row.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgba((theme.text_muted << 8) | 0xb3))
                                .child(description.clone()),
                        )
                    }),
            )
            .child(
                quick_command_action_button(&self.tokens, LucideIcon::Play).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.run_quick_command(&command_for_run, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                quick_command_action_button(&self.tokens, LucideIcon::Pencil).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.start_quick_command_edit(command_for_edit.clone(), cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                quick_command_action_button(&self.tokens, LucideIcon::Trash2).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.quick_commands.delete_command(&command_id);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_quick_command_category_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = self.quick_commands.category_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = !draft.name.trim().is_empty();
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
                            if let Some(draft) = this.quick_commands.category_editor.as_mut() {
                                draft.icon = icon;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        quick_command_lucide_icon(icon),
                        13.0,
                        if active {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .child(self.i18n.t(&quick_command_icon_label_key(icon))),
            );
        }

        div()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x59))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CategoryName,
                            draft.name.clone(),
                            self.i18n
                                .t("terminal.quick_commands.group_name_placeholder"),
                            cx,
                        ),
                    )
                    .child(icon_options),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save_group",
                |this, cx| this.save_quick_command_category_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_command_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = self.quick_commands.command_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = !draft.name.trim().is_empty() && !draft.command.trim().is_empty();
        let mut categories = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for category in &self.quick_commands.categories {
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
                            if let Some(draft) = this.quick_commands.command_editor.as_mut() {
                                draft.category = category_id.clone();
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(category.name.clone()),
            );
        }

        div()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x59))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandName,
                        draft.name.clone(),
                        self.i18n.t("terminal.quick_commands.name_placeholder"),
                        cx,
                    ))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandText,
                        draft.command.clone(),
                        self.i18n.t("terminal.quick_commands.command_placeholder"),
                        cx,
                    ))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandDescription,
                            draft.description.clone(),
                            self.i18n
                                .t("terminal.quick_commands.description_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandHostPattern,
                            draft.host_pattern.clone(),
                            self.i18n
                                .t("terminal.quick_commands.host_pattern_placeholder"),
                            cx,
                        ),
                    )
                    .child(categories),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save",
                |this, cx| this.save_quick_command_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_editor_buttons(
        &self,
        can_save: bool,
        save_key: &'static str,
        save: fn(&mut WorkspaceApp, &mut Context<WorkspaceApp>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                quick_command_text_button(
                    &self.tokens,
                    self.i18n.t("terminal.quick_commands.cancel"),
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.quick_commands.command_editor = None;
                        this.quick_commands.category_editor = None;
                        this.quick_commands.focused_input = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            )
            .child(
                quick_command_text_button(&self.tokens, self.i18n.t(save_key), can_save)
                    .bg(if can_save {
                        rgba((theme.accent << 8) | 0x26)
                    } else {
                        rgba(0x00000000)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if can_save {
                                save(this, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_quick_command_text_input(
        &self,
        input: QuickCommandInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.quick_commands.focused_input == Some(input);
        let target = WorkspaceImeTarget::QuickCommand(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(32.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.quick_commands.focused_input = Some(input);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection(target, event.position, event.modifiers.shift, window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn start_quick_command_create(&mut self, cx: &mut Context<Self>) {
        self.quick_commands.category_editor = None;
        self.quick_commands.command_editor = Some(QuickCommandDraft {
            id: None,
            name: String::new(),
            command: String::new(),
            category: self.quick_commands.active_category.clone(),
            description: String::new(),
            host_pattern: String::new(),
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CommandName);
        cx.notify();
    }

    fn start_quick_command_edit(&mut self, command: QuickCommand, cx: &mut Context<Self>) {
        self.quick_commands.category_editor = None;
        self.quick_commands.command_editor = Some(QuickCommandDraft {
            id: Some(command.id),
            name: command.name,
            command: command.command,
            category: command.category,
            description: command.description.unwrap_or_default(),
            host_pattern: command.host_pattern.unwrap_or_default(),
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CommandName);
        cx.notify();
    }

    fn start_quick_command_category_create(&mut self, cx: &mut Context<Self>) {
        self.quick_commands.command_editor = None;
        self.quick_commands.category_editor = Some(QuickCommandCategoryDraft {
            id: None,
            name: String::new(),
            icon: QuickCommandIcon::Zap,
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CategoryName);
        cx.notify();
    }

    fn start_quick_command_category_edit(
        &mut self,
        category: QuickCommandCategory,
        cx: &mut Context<Self>,
    ) {
        self.quick_commands.command_editor = None;
        self.quick_commands.category_editor = Some(QuickCommandCategoryDraft {
            id: Some(category.id),
            name: category.name,
            icon: category.icon,
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CategoryName);
        cx.notify();
    }

    fn save_quick_command_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.quick_commands.command_editor.take() else {
            return;
        };
        self.quick_commands.upsert_command(draft);
        self.quick_commands.focused_input = None;
        cx.notify();
    }

    fn save_quick_command_category_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.quick_commands.category_editor.take() else {
            return;
        };
        self.quick_commands.upsert_category(draft);
        self.quick_commands.focused_input = None;
        cx.notify();
    }
}
