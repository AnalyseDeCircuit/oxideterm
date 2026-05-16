impl WorkspaceApp {
    fn settings_keybindings(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let side = crate::keybindings::KeybindingSide::current();
        let modified = crate::keybindings::modified_count(&settings.keybindings.overrides);
        let query = self.keybinding_search_query.trim().to_lowercase();

        let mut rows = vec![
            self.keybinding_toolbar(modified, cx),
        ];

        let mut visible_scope_count = 0;
        for scope in [
            crate::keybindings::ActionScope::Global,
            crate::keybindings::ActionScope::Terminal,
            crate::keybindings::ActionScope::Split,
            crate::keybindings::ActionScope::Palette,
        ] {
            let definitions = crate::keybindings::ACTION_DEFINITIONS
                .iter()
                .filter(|definition| definition.scope == scope)
                .filter(|definition| self.keybinding_scope_filter.matches(definition.scope))
                .filter(|definition| {
                    if query.is_empty() {
                        return true;
                    }
                    let label = self.i18n.t(&definition.label_key()).to_lowercase();
                    label.contains(&query) || definition.id.to_lowercase().contains(&query)
                })
                .collect::<Vec<_>>();
            if !definitions.is_empty() {
                visible_scope_count += 1;
                rows.push(self.keybinding_scope_table(scope, &definitions, side, cx));
            }
        }

        if visible_scope_count == 0 {
            rows.push(self.keybinding_no_results());
        }

        rows.push(self.keybinding_reference_tables());

        rows
    }

    fn keybinding_toolbar(&self, modified: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("settings_view.keybindings.intl_keyboard_note")),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(self.keybinding_search_input(cx))
                    .child(self.keybinding_scope_filter(cx))
                    .child(div().flex_1().min_w(px(0.0)))
                    .child(self.keybinding_toolbar_button(
                        LucideIcon::Upload,
                        "settings_view.keybindings.import",
                        false,
                        cx,
                    ))
                    .child(self.keybinding_toolbar_button(
                        LucideIcon::Download,
                        "settings_view.keybindings.export",
                        false,
                        cx,
                    ))
                    .when(modified > 0, |toolbar| toolbar.child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.keybindings.reset_all"),
                            ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: modified == 0,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.keybinding_reset_all_confirm_open = true;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    )),
            )
            .into_any_element()
    }

    fn keybinding_search_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.focused_settings_input == Some(SettingsInput::KeybindingSearch);
        let value = if focused {
            self.settings_input_draft.as_str()
        } else {
            self.keybinding_search_query.as_str()
        };
        let target = WorkspaceImeTarget::Settings(SettingsInput::KeybindingSearch);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder: self.i18n.t("settings_view.keybindings.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(280.0))
            .h(px(36.0))
            .pl(px(34.0))
            .cursor(CursorStyle::IBeam)
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(10.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Search,
                        15.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.focus_settings_input(
                        SettingsInput::KeybindingSearch,
                        this.keybinding_search_query.clone(),
                        cx,
                    );
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
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

    fn keybinding_scope_filter(&self, cx: &mut Context<Self>) -> AnyElement {
        let filters = [
            KeybindingScopeFilter::All,
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Global),
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Terminal),
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Split),
            KeybindingScopeFilter::Scope(crate::keybindings::ActionScope::Palette),
        ];
        let mut row = div().flex().items_center().gap(px(4.0));
        for filter in filters {
            let active = self.keybinding_scope_filter == filter;
            row = row.child(
                button_with(
                    &self.tokens,
                    self.i18n.t(filter.label_key()),
                    ButtonOptions {
                        variant: if active {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        },
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.keybinding_scope_filter = filter;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            );
        }
        row.into_any_element()
    }

    fn keybinding_toolbar_button(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action = label_key;
        button_with(
            &self.tokens,
            self.i18n.t(label_key),
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .child(Self::render_lucide_icon(
            icon,
            14.0,
            rgb(self.tokens.ui.text_muted),
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| {
                match action {
                    "settings_view.keybindings.import" => this.import_keybindings(window, cx),
                    "settings_view.keybindings.export" => this.export_keybindings(cx),
                    _ => {}
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn keybinding_no_results(&self) -> AnyElement {
        div()
            .w_full()
            .py(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.keybindings.no_results"))
            .into_any_element()
    }

    fn keybinding_scope_table(
        &self,
        scope: crate::keybindings::ActionScope,
        definitions: &[&crate::keybindings::ActionDefinition],
        side: crate::keybindings::KeybindingSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut table = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .overflow_hidden()
            .child(
                div()
                    .h(px(40.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(theme.bg_panel))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(scope.label_key()).to_uppercase()),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.keybindings.column_shortcut")),
                    ),
            );

        for definition in definitions {
            table = table.child(self.keybinding_action_row(definition, side, cx));
        }

        table.into_any_element()
    }

    fn keybinding_action_row(
        &self,
        definition: &crate::keybindings::ActionDefinition,
        side: crate::keybindings::KeybindingSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let settings = self.settings_store.settings();
        let current =
            crate::keybindings::effective_combo(definition, &settings.keybindings.overrides, side);
        let default = definition.default_combo(side);
        let modified = current != *default;
        let recording = self
            .keybinding_recording_action_id
            .as_deref()
            .is_some_and(|id| id == definition.id);
        let action_id = definition.id.to_string();
        let record_action_id = action_id.clone();
        let reset_action_id = action_id.clone();
        let conflicts = if recording {
            self.keybinding_conflict_action_ids.as_slice()
        } else {
            &[]
        };

        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(14.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .border_b_1()
            .border_color(rgb(theme.border))
            .when(recording, |row| row.bg(rgba((theme.accent << 8) | 0x0d)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(&definition.label_key())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(definition.scope.label_key()))
                            .when(modified, |meta| {
                                meta.child(
                                    self.keybinding_modified_badge(),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(recording, |controls| {
                        controls.child(self.keybinding_recording_cell(conflicts, side, cx))
                    })
                    .when(!recording, |controls| {
                        controls
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .rounded(px(self.tokens.radii.sm))
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                                    .child(self.keybinding_kbd_badge(
                                        &crate::keybindings::format_combo(&current),
                                        false,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            this.keybinding_recording_action_id =
                                                Some(record_action_id.clone());
                                            this.keybinding_recording_combo = None;
                                            this.keybinding_conflict_action_ids.clear();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .when(modified, |controls| {
                                controls.child(
                                    self.keybinding_icon_button(LucideIcon::RotateCcw)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _event, window, cx| {
                                                this.reset_keybinding(&reset_action_id, window, cx);
                                                cx.stop_propagation();
                                            }),
                                        ),
                                )
                            })
                    }),
            )
            .into_any_element()
    }

    fn keybinding_recording_cell(
        &self,
        conflicts: &[String],
        side: crate::keybindings::KeybindingSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let combo = self.keybinding_recording_combo.as_ref();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(4.0))
                    .child(match combo {
                        Some(combo) => {
                            self.keybinding_kbd_badge(&crate::keybindings::format_combo(combo), true)
                        }
                        None => div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.keybindings.record_prompt"))
                            .into_any_element(),
                    })
                    .when(combo.is_some() && !conflicts.is_empty(), |cell| {
                        cell.child(
                            div()
                                .max_w(px(240.0))
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgb(theme.warning))
                                .child(self.keybinding_conflict_text(conflicts, side)),
                        )
                    }),
            )
            .when(combo.is_some(), |cell| {
                let label_key = if conflicts.is_empty() {
                    "✓"
                } else {
                    "settings_view.keybindings.override_anyway"
                };
                cell.child(
                    button_with(
                        &self.tokens,
                        if conflicts.is_empty() {
                            label_key.to_string()
                        } else {
                            self.i18n.t(label_key)
                        },
                        ButtonOptions {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.confirm_keybinding_recording(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .child(
                self.keybinding_icon_button(LucideIcon::X).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.cancel_keybinding_recording(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn keybinding_modified_badge(&self) -> AnyElement {
        div()
            .flex_none()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x33))
            .px(px(6.0))
            .py(px(1.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.i18n.t("settings_view.keybindings.modified"))
            .into_any_element()
    }

    fn keybinding_kbd_badge(&self, value: &str, accent: bool) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if accent {
                rgba((self.tokens.ui.accent << 8) | 0x4d)
            } else {
                rgba((self.tokens.ui.border << 8) | 0x80)
            })
            .bg(if accent {
                rgba((self.tokens.ui.accent << 8) | 0x33)
            } else {
                rgb(self.tokens.ui.bg)
            })
            .px(px(8.0))
            .py(px(2.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(if accent {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text
            }))
            .child(value.to_string())
            .into_any_element()
    }

    fn keybinding_icon_button(&self, icon: LucideIcon) -> Div {
        div()
            .size(px(28.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(self.tokens.ui.text_muted))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
    }

    fn keybinding_conflict_text(
        &self,
        conflicts: &[String],
        side: crate::keybindings::KeybindingSide,
    ) -> String {
        let Some(conflict) = conflicts
            .iter()
            .filter_map(|id| crate::keybindings::action_definition(id))
            .next()
        else {
            return String::new();
        };
        self.i18n
            .t("settings_view.keybindings.conflict_warning")
            .replace("{{scope}}", &self.i18n.t(conflict.scope.label_key()))
            .replace("{{action}}", &self.i18n.t(&conflict.label_key()))
            .replace(
                "{{shortcut}}",
                &crate::keybindings::format_combo(conflict.default_combo(side)),
            )
    }

    fn render_keybinding_reset_all_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.i18n.t("settings_view.keybindings.reset_all_confirm"))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("settings_view.keybindings.reset_all"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.keybinding_reset_all_confirm_open = false;
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, window, cx| {
                this.keybinding_reset_all_confirm_open = false;
                this.reset_all_keybindings(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn keybinding_reference_tables(&self) -> AnyElement {
        let categories = [
            (
                "settings_view.help.category_file_manager",
                vec![
                    ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                    ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                    ("settings_view.help.shortcut_cut", "⌘X", "Ctrl+X"),
                    ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                    ("settings_view.help.shortcut_rename", "F2", "F2"),
                    ("settings_view.help.shortcut_delete", "Delete", "Delete"),
                    ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                    ("settings_view.help.shortcut_open", "Enter", "Enter"),
                ],
            ),
            (
                "settings_view.help.category_sftp",
                vec![
                    ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                    ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                    ("settings_view.help.shortcut_sftp_enter_dir", "Enter", "Enter"),
                    ("settings_view.help.shortcut_sftp_upload", "→", "→"),
                    ("settings_view.help.shortcut_sftp_download", "←", "←"),
                    ("settings_view.help.shortcut_rename", "F2", "F2"),
                    ("settings_view.help.shortcut_delete", "Delete", "Delete"),
                ],
            ),
            (
                "settings_view.help.category_editor",
                vec![
                    ("settings_view.help.shortcut_save", "⌘S", "Ctrl+S"),
                    ("settings_view.help.shortcut_find", "⌘F", "Ctrl+F"),
                    ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                    ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                    ("settings_view.help.shortcut_close", "Esc", "Esc"),
                ],
            ),
        ];

        let theme = self.tokens.ui;
        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .pt(px(6.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("settings_view.help.shortcuts")),
            );

        for (title_key, rows) in categories {
            let mut table = div()
                .w_full()
                .min_w(px(0.0))
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .overflow_hidden()
                .child(
                    div()
                        .h(px(36.0))
                        .px(px(14.0))
                        .flex()
                        .items_center()
                        .bg(rgb(theme.bg_panel))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t(title_key).to_uppercase()),
                );
            let row_count = rows.len();
            for (index, (label_key, mac, other)) in rows.into_iter().enumerate() {
                table = table.child(self.keybinding_reference_row(
                    label_key,
                    mac,
                    other,
                    index + 1 < row_count,
                ));
            }
            section = section.child(table);
        }

        section.into_any_element()
    }

    fn keybinding_reference_row(
        &self,
        label_key: &str,
        mac: &str,
        other: &str,
        show_separator: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(14.0))
            .py(px(9.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .when(show_separator, |row| {
                row.border_b_1().border_color(rgb(theme.border))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.keybinding_badge(mac))
            .child(self.keybinding_badge(other))
            .into_any_element()
    }

    fn keybinding_badge(&self, value: &str) -> AnyElement {
        div()
            .flex_none()
            .min_w(px(72.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .flex()
            .items_center()
            .justify_center()
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .child(value.to_string())
            .into_any_element()
    }

}
