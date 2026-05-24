#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeybindingToolbarAction {
    Import,
    Export,
    ResetAll,
}

const KEYBINDING_SCOPE_FILTER_HEIGHT: f32 = 32.0; // Tauri KeybindingEditorSection scope Button h-8
const KEYBINDING_SCOPE_FILTER_PADDING_X: f32 = 12.0; // Tauri px-3

impl KeybindingToolbarAction {
    fn label_key(self) -> &'static str {
        match self {
            Self::Import => "settings_view.keybindings.import",
            Self::Export => "settings_view.keybindings.export",
            Self::ResetAll => "settings_view.keybindings.reset_all",
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::Import => LucideIcon::Upload,
            Self::Export => LucideIcon::Download,
            Self::ResetAll => LucideIcon::RotateCcw,
        }
    }

    fn destructive(self) -> bool {
        matches!(self, Self::ResetAll)
    }
}

impl WorkspaceApp {
    fn settings_keybindings_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let modified = crate::keybindings::modified_count(&settings.keybindings.overrides);
        if section_index == 0 {
            return self.keybinding_toolbar(modified, cx);
        }

        let side = crate::keybindings::KeybindingSide::current();
        let query = self.keybinding_search_query.trim().to_lowercase();
        let mut visible_index = 0;
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
                visible_index += 1;
                if visible_index == section_index {
                    return self.keybinding_scope_table(scope, &definitions, side, cx);
                }
            }
        }

        if section_index == 1 && visible_index == 0 {
            return self.keybinding_no_results(cx);
        }

        div().into_any_element()
    }

    fn keybinding_toolbar(&self, modified: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
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
                        KeybindingToolbarAction::Import,
                        false,
                        cx,
                    ))
                    .child(self.keybinding_toolbar_button(
                        KeybindingToolbarAction::Export,
                        false,
                        cx,
                    ))
                    .when(modified > 0, |toolbar| toolbar.child(
                        self.keybinding_toolbar_button(
                            KeybindingToolbarAction::ResetAll,
                            modified == 0,
                            cx,
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
                    selected_range: self.ime_selected_range_for_target(target),
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
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.focus_settings_input(
                        SettingsInput::KeybindingSearch,
                        this.keybinding_search_query.clone(),
                        cx,
                    );
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
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
            row = row.child(self.keybinding_scope_filter_button(filter, active, cx));
        }
        row.into_any_element()
    }

    fn keybinding_scope_filter_button(
        &self,
        filter: KeybindingScopeFilter,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        // Tauri renders these as compact shadcn Buttons (`h-8 px-3 text-xs`).
        // Route activation through the workspace wrapper so scope pills share
        // the same disabled/loading click guard as every other settings Button.
        self.workspace_toolbar_action_button(
            self.i18n.t(filter.label_key()),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if active {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                height: Some(KEYBINDING_SCOPE_FILTER_HEIGHT),
                padding_x: Some(KEYBINDING_SCOPE_FILTER_PADDING_X),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.keybinding_scope_filter = filter;
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    fn keybinding_toolbar_button(
        &self,
        action: KeybindingToolbarAction,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_color = if action.destructive() {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text_muted
        };
        let hover_text_color = if action.destructive() {
            rgb(self.tokens.ui.error)
        } else {
            rgb(self.tokens.ui.text)
        };

        // Tauri renders keybinding toolbar actions as shadcn ghost Buttons
        // with leading lucide icons. Use the workspace action guard so disabled
        // toolbar buttons cannot dispatch pointer activation in native either.
        self.workspace_toolbar_action_button(
            self.i18n.t(action.label_key()),
            Some(Self::render_lucide_icon(
                action.icon(),
                14.0,
                rgb(icon_color),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                text_color: action.destructive().then(|| rgb(self.tokens.ui.error)),
                hover_text_color: Some(hover_text_color),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                match action {
                    KeybindingToolbarAction::Import => this.import_keybindings(window, cx),
                    KeybindingToolbarAction::Export => this.export_keybindings(cx),
                    KeybindingToolbarAction::ResetAll => {
                        this.keybinding_reset_all_confirm_open = true;
                        this.reset_standard_confirm_focus();
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn keybinding_no_results(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .py(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "settings-keybindings",
                "no-results",
                self.i18n.t("settings_view.keybindings.no_results"),
                self.tokens.ui.text_muted,
                cx,
            ))
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
            .bg(rgb(theme.bg_card))
            .overflow_hidden()
            .child(
                div()
                    .h(px(40.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    // Tauri relies on `overflow-hidden` on the rounded table.
                    // GPUI can leave child paint visible at the mask edge, so
                    // round the header explicitly to preserve the browser clip.
                    .rounded_t(px(self.tokens.radii.lg))
                    .bg(rgba((theme.bg_panel << 8) | 0x80))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "settings-keybindings-scope",
                                scope.label_key(),
                                self.i18n.t(scope.label_key()).to_uppercase(),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "settings-keybindings-column",
                                "shortcut",
                                self.i18n.t("settings_view.keybindings.column_shortcut"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            );

        for (index, definition) in definitions.iter().enumerate() {
            table = table.child(self.keybinding_action_row(
                definition,
                side,
                index + 1 == definitions.len(),
                cx,
            ));
        }

        table.into_any_element()
    }

    fn keybinding_action_row(
        &self,
        definition: &crate::keybindings::ActionDefinition,
        side: crate::keybindings::KeybindingSide,
        is_last: bool,
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
            .px(px(20.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            // Tauri's `divide-y` does not draw a divider after the last row.
            // Avoid a final border inside the rounded bottom because GPUI's
            // rounded overflow can otherwise expose the line outside the mask.
            .when(!is_last, |row| {
                row.border_b_1().border_color(rgb(theme.border))
            })
            .when(is_last, |row| row.rounded_b(px(self.tokens.radii.lg)))
            .when(recording, |row| row.bg(rgba((theme.accent << 8) | 0x0d)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "settings-keybinding-action",
                                definition.id,
                                self.i18n.t(&definition.label_key()),
                                theme.text,
                                cx,
                            )),
                    )
                    .when(modified, |label| label.child(self.keybinding_modified_badge(cx))),
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
                                        cx,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            this.keybinding_recording_action_id =
                                                Some(record_action_id.clone());
                                            this.keybinding_recording_combo = None;
                                            this.keybinding_recording_footer_focus = None;
                                            this.keybinding_conflict_action_ids.clear();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .when(modified, |controls| {
                                controls.child(self.workspace_icon_action_button(
                                    LucideIcon::RotateCcw,
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                    IconButtonOptions {
                                        hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                                        // KeybindingEditorSection renders this
                                        // as a compact icon Button. Use the
                                        // shared icon guard instead of a local
                                        // wrapper so reset actions cannot drift.
                                        ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                    },
                                    move |this, _event, window, cx| {
                                        this.reset_keybinding(&reset_action_id, window, cx);
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
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
                            self.keybinding_kbd_badge(&crate::keybindings::format_combo(combo), true, cx)
                        }
                        None => div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "settings-keybindings-record",
                                "prompt",
                                self.i18n.t("settings_view.keybindings.record_prompt"),
                                theme.text_muted,
                                cx,
                            ))
                            .into_any_element(),
                    })
                    .when(combo.is_some() && !conflicts.is_empty(), |cell| {
                        cell.child(
                            div()
                                .max_w(px(240.0))
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgb(theme.warning))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "settings-keybindings-conflict",
                                    conflicts.join("|"),
                                    self.keybinding_conflict_text(conflicts, side),
                                    theme.warning,
                                    cx,
                                )),
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
                    self.keybinding_recording_confirm_button(
                        if conflicts.is_empty() {
                            label_key.to_string()
                        } else {
                            self.i18n.t(label_key)
                        },
                        conflicts.is_empty(),
                        cx,
                    )
                )
            })
            .child(self.keybinding_recording_cancel_button(cx))
            .into_any_element()
    }

    fn keybinding_modified_badge(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_none()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x33))
            .px(px(6.0))
            .py(px(1.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "settings-keybindings",
                "modified",
                self.i18n.t("settings_view.keybindings.modified"),
                self.tokens.ui.accent,
                cx,
            ))
            .into_any_element()
    }

    fn keybinding_kbd_badge(
        &self,
        value: &str,
        accent: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "settings-keybinding-chip",
                (value, accent),
                value.to_string(),
                if accent {
                    self.tokens.ui.accent
                } else {
                    self.tokens.ui.text
                },
                cx,
            ))
            .into_any_element()
    }

    fn keybinding_recording_confirm_button(
        &self,
        label: String,
        is_clean_combo: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text_color = if is_clean_combo {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.warning
        };

        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                text_color: Some(rgb(text_color)),
                hover_text_color: Some(rgb(text_color)),
                // Browser focus rings on Tauri's RecordingCell buttons are
                // keyboard-origin only; mouse activation below clears this
                // footer focus instead of synthesizing a focus-visible state.
                focus_visible: self.keybinding_recording_footer_focus
                    == Some(KeybindingRecordingFooterAction::Confirm),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, window, cx| {
                this.activate_keybinding_recording_footer_action(
                    KeybindingRecordingFooterAction::Confirm,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn keybinding_recording_cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.workspace_icon_action_button(
            LucideIcon::X,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                focus_visible: self.keybinding_recording_footer_focus
                    == Some(KeybindingRecordingFooterAction::Cancel),
                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
            },
            |this, _event, window, cx| {
                this.activate_keybinding_recording_footer_action(
                    KeybindingRecordingFooterAction::Cancel,
                    window,
                    cx,
                );
                cx.stop_propagation();
            },
            cx,
        )
        .into_any_element()
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
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "settings-keybindings-reset-dialog",
                        "title",
                        self.i18n.t("settings_view.keybindings.reset_all_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    // Dialog action labels mirror browser select-none buttons.
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "settings-keybindings-reset-dialog",
                        "cancel",
                        self.i18n.t("common.actions.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "settings-keybindings-reset-dialog",
                        "confirm",
                        self.i18n.t("settings_view.keybindings.reset_all"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.keybinding_reset_all_confirm_open = false;
                this.clear_standard_confirm_focus();
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, window, cx| {
                this.keybinding_reset_all_confirm_open = false;
                this.clear_standard_confirm_focus();
                this.reset_all_keybindings(window, cx);
                cx.stop_propagation();
            }),
        )
    }

}
