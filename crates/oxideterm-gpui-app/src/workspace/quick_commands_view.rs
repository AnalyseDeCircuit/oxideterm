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

fn close_terminal_quick_commands_popover_state(
    open: &mut bool,
    pending_command: &mut Option<String>,
    focused_input: &mut Option<QuickCommandInput>,
    highlighted_command: &mut Option<String>,
) {
    *open = false;
    *pending_command = None;
    *focused_input = None;
    *highlighted_command = None;
}

fn insert_quick_command_into_command_bar_state(
    draft: &mut String,
    command: &str,
    command_bar_focused: &mut bool,
    open: &mut bool,
    pending_command: &mut Option<String>,
    focused_input: &mut Option<QuickCommandInput>,
    highlighted_command: &mut Option<String>,
) {
    *draft = command.to_string();
    *command_bar_focused = true;
    close_terminal_quick_commands_popover_state(
        open,
        pending_command,
        focused_input,
        highlighted_command,
    );
}

fn quick_command_draft_can_save(draft: &QuickCommandDraft) -> bool {
    !draft.name.trim().is_empty() && !draft.command.trim().is_empty()
}

fn quick_command_category_draft_can_save(draft: &QuickCommandCategoryDraft) -> bool {
    !draft.name.trim().is_empty()
}

fn quick_command_editor_tab_target(
    input: QuickCommandInput,
    forward: bool,
) -> Option<QuickCommandInput> {
    // Tauri quick command editors use ordinary DOM focus, so Tab/Shift+Tab
    // walks editable fields in source order. GPUI currently only models the
    // text-field focus targets here, so cycle that subset instead of letting
    // the root focused-input capture swallow Tab at the editor edges.
    const COMMAND_EDITOR_FIELDS: &[QuickCommandInput] = &[
        QuickCommandInput::CommandName,
        QuickCommandInput::CommandText,
        QuickCommandInput::CommandDescription,
        QuickCommandInput::CommandHostPattern,
    ];
    let index = COMMAND_EDITOR_FIELDS
        .iter()
        .position(|candidate| *candidate == input)?;
    if forward {
        COMMAND_EDITOR_FIELDS
            .get(index + 1)
            .copied()
            .or_else(|| COMMAND_EDITOR_FIELDS.first().copied())
    } else {
        index
            .checked_sub(1)
            .and_then(|previous| COMMAND_EDITOR_FIELDS.get(previous).copied())
            .or_else(|| COMMAND_EDITOR_FIELDS.last().copied())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuickCommandKeyDirection {
    Next,
    Previous,
}

fn quick_command_highlighted_index(
    visible_commands: &[QuickCommand],
    highlighted_command: Option<&str>,
) -> Option<usize> {
    highlighted_command.and_then(|id| {
        visible_commands
            .iter()
            .position(|command| command.id.as_str() == id)
    })
}

fn quick_command_keyboard_highlight(
    visible_commands: &[QuickCommand],
    highlighted_command: Option<&str>,
    direction: QuickCommandKeyDirection,
) -> Option<String> {
    if visible_commands.is_empty() {
        return None;
    }
    let current = quick_command_highlighted_index(visible_commands, highlighted_command);
    let next = match direction {
        QuickCommandKeyDirection::Next => current
            .map(|index| (index + 1).min(visible_commands.len() - 1))
            .unwrap_or(0),
        QuickCommandKeyDirection::Previous => current
            .map(|index| index.saturating_sub(1))
            .unwrap_or(visible_commands.len() - 1),
    };
    Some(visible_commands[next].id.clone())
}

fn quick_command_highlight_at(visible_commands: &[QuickCommand], index: usize) -> Option<String> {
    visible_commands
        .get(index.min(visible_commands.len().saturating_sub(1)))
        .map(|command| command.id.clone())
}

impl WorkspaceApp {
    fn visible_quick_commands_for_active_terminal(&self) -> Vec<QuickCommand> {
        let active_label = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_default();
        self.quick_commands
            .visible_commands_for_targets(&[active_label])
    }

    pub(super) fn close_terminal_quick_commands_popover(&mut self) {
        close_terminal_quick_commands_popover_state(
            &mut self.terminal_quick_commands_open,
            &mut self.terminal_quick_command_pending,
            &mut self.quick_commands.focused_input,
            &mut self.quick_commands.highlighted_command,
        );
    }

    fn insert_quick_command_into_command_bar(&mut self, command: &str) {
        insert_quick_command_into_command_bar_state(
            &mut self.terminal_command_bar_draft,
            command,
            &mut self.terminal_command_bar_focused,
            &mut self.terminal_quick_commands_open,
            &mut self.terminal_quick_command_pending,
            &mut self.quick_commands.focused_input,
            &mut self.quick_commands.highlighted_command,
        );
    }

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
        if input == QuickCommandInput::Search {
            match key {
                "escape" if !modifiers.platform && !modifiers.control => {
                    // Tauri keeps Escape as the browser-like popover dismissal
                    // path for the Command Bar quick commands surface.
                    self.close_terminal_quick_commands_popover();
                    self.terminal_command_bar_focused = true;
                    self.ime_marked_text = None;
                    cx.notify();
                    return;
                }
                "arrowdown" | "down" if !modifiers.platform && !modifiers.control => {
                    let visible_commands = self.visible_quick_commands_for_active_terminal();
                    self.quick_commands.highlighted_command = quick_command_keyboard_highlight(
                        &visible_commands,
                        self.quick_commands.highlighted_command.as_deref(),
                        QuickCommandKeyDirection::Next,
                    );
                    cx.notify();
                    return;
                }
                "arrowup" | "up" if !modifiers.platform && !modifiers.control => {
                    let visible_commands = self.visible_quick_commands_for_active_terminal();
                    self.quick_commands.highlighted_command = quick_command_keyboard_highlight(
                        &visible_commands,
                        self.quick_commands.highlighted_command.as_deref(),
                        QuickCommandKeyDirection::Previous,
                    );
                    cx.notify();
                    return;
                }
                "home" if !modifiers.platform && !modifiers.control => {
                    let visible_commands = self.visible_quick_commands_for_active_terminal();
                    self.quick_commands.highlighted_command =
                        quick_command_highlight_at(&visible_commands, 0);
                    cx.notify();
                    return;
                }
                "end" if !modifiers.platform && !modifiers.control => {
                    let visible_commands = self.visible_quick_commands_for_active_terminal();
                    self.quick_commands.highlighted_command =
                        visible_commands.last().map(|command| command.id.clone());
                    cx.notify();
                    return;
                }
                "enter" if !modifiers.platform && !modifiers.control => {
                    let visible_commands = self.visible_quick_commands_for_active_terminal();
                    let selected_index = quick_command_highlighted_index(
                        &visible_commands,
                        self.quick_commands.highlighted_command.as_deref(),
                    )
                    .unwrap_or(0);
                    if let Some(command) = visible_commands.get(selected_index) {
                        let command_text = command.command.clone();
                        self.insert_quick_command_into_command_bar(&command_text);
                        cx.notify();
                    }
                    return;
                }
                _ => {}
            }
        }
        match key {
            "tab" if !modifiers.platform && !modifiers.control => {
                if self.quick_commands.command_editor.is_some()
                    && let Some(next_input) =
                        quick_command_editor_tab_target(input, !modifiers.shift)
                {
                    self.quick_commands.focused_input = Some(next_input);
                    self.clear_ime_selection();
                    self.ime_marked_text = None;
                    cx.notify();
                }
            }
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
                if input == QuickCommandInput::Search {
                    self.quick_commands.highlighted_command = None;
                }
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
        let visible_commands = self.visible_quick_commands_for_active_terminal();
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
            })
            .on_scroll_wheel(|_, _, cx| {
                // Match Tauri's popover scroll boundary: wheel input inside
                // the quick command surface must not close the overlay or leak
                // to the terminal behind it.
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "quick-commands",
                                "title",
                                self.i18n.t("terminal.quick_commands.title").to_uppercase(),
                                theme.text_muted,
                                cx,
                            )),
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
                                            this.close_terminal_quick_commands_popover();
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
                                        this.quick_commands.highlighted_command = None;
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
                            .child(div().flex_1().truncate().child(
                                self.render_row_safe_selectable_display_text_in_group(
                                    crate::workspace::selectable_text::selectable_text_id(
                                        "quick-command-category-row",
                                        &category.id,
                                    ),
                                    "quick-command-category-cell",
                                    ("name", category.id.as_str()),
                                    0,
                                    category.name.clone(),
                                    if active { theme.accent } else { theme.text_muted },
                                    None,
                                    cx,
                                ),
                            ))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.md))
                                    .bg(rgb(theme.bg_panel))
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::NonSelectable,
                                        "quick-command-category-count",
                                        category.id.as_str(),
                                        count.to_string(),
                                        theme.text_muted,
                                        cx,
                                    )),
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
                                            this.quick_commands.highlighted_command = None;
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
                            // Tauri treats this as a select-none control label; selection must not steal the button click.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "quick-command-add-button",
                                "label",
                                self.i18n.t("terminal.quick_commands.add"),
                                theme.text_muted,
                                cx,
                            )),
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
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "quick-commands-empty",
                    self.quick_commands.query.as_str(),
                    if self.quick_commands.query.trim().is_empty() {
                        self.i18n.t("terminal.quick_commands.empty_category")
                    } else {
                        self.i18n.t("terminal.quick_commands.empty_search")
                    },
                    theme.text_muted,
                    cx,
                ))
                .into_any_element();
        }

        let mut list = div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scrollbar()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
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
        let command_id_for_hover = command.id.clone();
        let highlighted =
            self.quick_commands.highlighted_command.as_deref() == Some(command.id.as_str());
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "quick-command-row",
            command.id.as_str(),
        );
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
                    if this.quick_commands.highlighted_command.as_deref()
                        != Some(command_id_for_hover.as_str())
                    {
                        this.quick_commands.highlighted_command = Some(command_id_for_hover.clone());
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
                            this.insert_quick_command_into_command_bar(&command_for_insert);
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
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::NonSelectable,
                                            "quick-command-risk",
                                            command.id.as_str(),
                                            risk.to_uppercase(),
                                            if risk == "high" {
                                                0xfca5a5
                                            } else {
                                                0xfcd34d
                                            },
                                            cx,
                                        )),
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
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::NonSelectable,
                                            "quick-command-host-pattern",
                                            command.id.as_str(),
                                            pattern.clone(),
                                            theme.text_muted,
                                            cx,
                                        )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
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
                                .text_size(px(11.0))
                                .text_color(rgba((theme.text_muted << 8) | 0xb3))
                                .child(self.render_row_safe_selectable_display_text_in_group_with_alpha(
                                    selection_group_id,
                                    "quick-command-row-cell",
                                    ("description", command.id.as_str()),
                                    2,
                                    description.clone(),
                                    theme.text_muted,
                                    0xb3 as f32 / 255.0,
                                    None,
                                    cx,
                                )),
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
                        this.quick_commands.highlighted_command = None;
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
        let can_save = quick_command_category_draft_can_save(draft);
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-icon-option",
                        icon.as_source_id(),
                        self.i18n.t(&quick_command_icon_label_key(icon)),
                        if active { theme.accent } else { theme.text_muted },
                        cx,
                    )),
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
        let can_save = quick_command_draft_can_save(draft);
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-editor-category",
                        category.id.as_str(),
                        category.name.clone(),
                        if active { theme.accent } else { theme.text_muted },
                        cx,
                    )),
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
                        this.quick_commands.highlighted_command = None;
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
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
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
        self.quick_commands.highlighted_command = None;
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
        self.quick_commands.highlighted_command = None;
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
        self.quick_commands.highlighted_command = None;
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
        self.quick_commands.highlighted_command = None;
        cx.notify();
    }

    fn save_quick_command_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.quick_commands.command_editor.as_ref() else {
            return;
        };
        if !quick_command_draft_can_save(draft) {
            return;
        }
        let Some(draft) = self.quick_commands.command_editor.take() else {
            return;
        };
        self.quick_commands.upsert_command(draft);
        self.quick_commands.focused_input = None;
        self.quick_commands.highlighted_command = None;
        cx.notify();
    }

    fn save_quick_command_category_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.quick_commands.category_editor.as_ref() else {
            return;
        };
        if !quick_command_category_draft_can_save(draft) {
            return;
        }
        let Some(draft) = self.quick_commands.category_editor.take() else {
            return;
        };
        self.quick_commands.upsert_category(draft);
        self.quick_commands.focused_input = None;
        self.quick_commands.highlighted_command = None;
        cx.notify();
    }
}

#[cfg(test)]
mod terminal_command_bar_quick_command_tests {
    use super::*;

    #[test]
    fn quick_command_popover_outside_click_closes_without_blurring_command_bar() {
        let mut open = true;
        let mut pending_command = Some("rm -rf /tmp/example".to_string());
        let mut focused_input = Some(QuickCommandInput::Search);
        let mut highlighted_command = Some("qc-risky".to_string());
        let command_bar_focused = true;

        close_terminal_quick_commands_popover_state(
            &mut open,
            &mut pending_command,
            &mut focused_input,
            &mut highlighted_command,
        );

        assert!(!open);
        assert_eq!(pending_command, None);
        assert_eq!(focused_input, None);
        assert_eq!(highlighted_command, None);
        assert!(command_bar_focused);
    }

    #[test]
    fn quick_command_row_click_inserts_command_and_keeps_submit_closed() {
        let mut draft = String::new();
        let mut command_bar_focused = false;
        let mut open = true;
        let mut pending_command = Some("docker system prune".to_string());
        let mut focused_input = Some(QuickCommandInput::Search);
        let mut highlighted_command = Some("qc-docker".to_string());

        insert_quick_command_into_command_bar_state(
            &mut draft,
            "git status",
            &mut command_bar_focused,
            &mut open,
            &mut pending_command,
            &mut focused_input,
            &mut highlighted_command,
        );

        assert_eq!(draft, "git status");
        assert!(command_bar_focused);
        assert!(!open);
        assert_eq!(pending_command, None);
        assert_eq!(focused_input, None);
        assert_eq!(highlighted_command, None);
    }

    #[test]
    fn quick_command_keyboard_highlight_clamps_like_browser_menu_focus() {
        let commands = vec![
            QuickCommand {
                id: "first".to_string(),
                name: "First".to_string(),
                command: "pwd".to_string(),
                category: "system".to_string(),
                description: None,
                host_pattern: None,
                created_at: 0,
                updated_at: 0,
            },
            QuickCommand {
                id: "second".to_string(),
                name: "Second".to_string(),
                command: "ls".to_string(),
                category: "system".to_string(),
                description: None,
                host_pattern: None,
                created_at: 0,
                updated_at: 0,
            },
        ];

        assert_eq!(
            quick_command_keyboard_highlight(&commands, None, QuickCommandKeyDirection::Next),
            Some("first".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(
                &commands,
                Some("first"),
                QuickCommandKeyDirection::Next
            ),
            Some("second".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(
                &commands,
                Some("second"),
                QuickCommandKeyDirection::Next
            ),
            Some("second".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(&commands, None, QuickCommandKeyDirection::Previous),
            Some("second".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(
                &commands,
                Some("missing"),
                QuickCommandKeyDirection::Next
            ),
            Some("first".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(&[], None, QuickCommandKeyDirection::Next),
            None
        );
    }

    #[test]
    fn quick_command_editor_tab_cycles_text_fields_without_swallowing_focus() {
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::CommandName, true),
            Some(QuickCommandInput::CommandText)
        );
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::CommandHostPattern, true),
            Some(QuickCommandInput::CommandName)
        );
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::CommandName, false),
            Some(QuickCommandInput::CommandHostPattern)
        );
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::Search, true),
            None
        );
    }

    #[test]
    fn quick_command_editor_save_gate_matches_tauri_disabled_button() {
        assert!(!quick_command_draft_can_save(&QuickCommandDraft {
            id: None,
            name: String::new(),
            command: "git status".to_string(),
            category: "system".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        }));
        assert!(!quick_command_draft_can_save(&QuickCommandDraft {
            id: None,
            name: "Status".to_string(),
            command: "   ".to_string(),
            category: "system".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        }));
        assert!(quick_command_draft_can_save(&QuickCommandDraft {
            id: None,
            name: "Status".to_string(),
            command: "git status".to_string(),
            category: "system".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        }));
    }

    #[test]
    fn quick_command_category_editor_save_gate_matches_tauri_disabled_button() {
        assert!(!quick_command_category_draft_can_save(
            &QuickCommandCategoryDraft {
                id: None,
                name: "   ".to_string(),
                icon: QuickCommandIcon::Zap,
            }
        ));
        assert!(quick_command_category_draft_can_save(
            &QuickCommandCategoryDraft {
                id: None,
                name: "Ops".to_string(),
                icon: QuickCommandIcon::Zap,
            }
        ));
    }
}
