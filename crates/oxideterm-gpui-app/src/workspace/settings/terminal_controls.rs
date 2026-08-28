use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn number_input(
        &self,
        input: SettingsInput,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        // Numeric settings keep centered text while sharing the same input
        // ownership and selection behavior as other Workspace controls.
        self.text_input_with_workspace_ime(
            target,
            text_input_with_content_align(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder: value.clone(),
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
                TextInputContentAlign::Center,
            )
            .w(px(width)),
            move |this, cx| {
                let current = this.current_settings_input_value(input, cx);
                this.focus_settings_input(input, current, cx);
            },
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn font_size_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slider_view = SliderView {
            min: 8.0,
            max: 32.0,
            value: settings.terminal.font_size as f32,
            disabled: false,
        };
        let workspace = cx.entity();
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(self.tokens.metrics.settings_slider_width))
                    .child(select_anchor_probe(
                        SelectAnchorId::SettingsTerminalFontSizeSlider,
                        slider(&self.tokens, slider_view)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    this.close_settings_select();
                                    this.focused_settings_input = None;
                                    this.settings_slider_drag =
                                        Some(SettingsSlider::TerminalFontSize);
                                    this.set_font_size_from_position(
                                        f32::from(event.position.x),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                                    this.finish_settings_slider_drag(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                |this, event: &MouseMoveEvent, _window, cx| {
                                    this.update_settings_slider_drag(event, cx);
                                },
                            )),
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_select_anchor(anchor, cx);
                            });
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.number_input(
                        SettingsInput::TerminalFontSize,
                        settings.terminal.font_size.to_string(),
                        self.tokens.metrics.settings_font_size_input_width,
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("px"),
                    ),
            )
            .into_any_element();

        self.setting_row(
            "settings_view.terminal.font_size",
            "settings_view.terminal.font_size_hint",
            control,
            cx,
        )
    }

    pub(in crate::workspace) fn decimal_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.number_input(
                input,
                value,
                self.tokens.metrics.settings_number_input_width,
                cx,
            ),
            cx,
        )
    }

    pub(in crate::workspace) fn checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
            cx,
        )
    }

    pub(in crate::workspace) fn settings_text_input_control(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            Some(width),
            TextInputContentAlign::Start,
            false,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_text_input_control_fill(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Responsive form fields own their width at the parent flex layer.
        // Filling that slot avoids fixed-width inputs inside growing columns.
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            None,
            TextInputContentAlign::Start,
            false,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_secret_text_input_control(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            Some(width),
            TextInputContentAlign::Start,
            true,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_secret_text_input_control_fill(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Secret controls use the same responsive slot contract as normal inputs.
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            None,
            TextInputContentAlign::Start,
            true,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_text_input_control_with_align(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        align: TextInputContentAlign,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_text_input_control_inner(
            input,
            value,
            placeholder,
            Some(width),
            align,
            false,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_text_input_control_inner(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: Option<f32>,
        align: TextInputContentAlign,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings_workspace = self.settings_workspace.read(cx);
        let entity_value = settings_workspace.settings_entity_input_value(input);
        let entity_owned = entity_value.is_some();
        let focused = if entity_owned {
            settings_workspace.settings_entity_focused_input() == Some(input)
        } else {
            self.focused_settings_input == Some(input)
        };
        let display_value = if let Some(entity_value) = entity_value {
            // Borrow Entity-owned drafts for this frame instead of cloning them
            // into root rendering snapshots, especially for secret inputs.
            entity_value
        } else if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_ref()
        };
        let target = WorkspaceImeTarget::Settings(input);
        self.text_input_with_workspace_ime(
            target,
            text_input_with_content_align(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    // Managed-key passphrases mirror Tauri password inputs; the
                    // shared settings input pipeline still owns focus and IME.
                    secret,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
                align,
            )
            .when_some(width, |input, width| input.w(px(width)).max_w_full())
            .when(width.is_none(), |input| input.w_full())
            .min_w(px(0.0)),
            move |this, cx| {
                let current = this.current_settings_input_value(input, cx);
                this.focus_settings_input(input, current, cx);
            },
            cx,
        )
        .into_any_element()
    }
}
