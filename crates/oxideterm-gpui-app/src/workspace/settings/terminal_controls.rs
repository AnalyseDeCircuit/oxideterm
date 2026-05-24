impl WorkspaceApp {
    fn number_input(
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
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input_with_content_align(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder: value.clone(),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
                TextInputContentAlign::Center,
            )
            .w(px(width))
            // Tauri number inputs use the browser input text box; keep numeric
            // settings values centered in the padded field instead of inheriting
            // the old right-aligned GPUI flex row.
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
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

    fn font_size_row(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
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

    fn decimal_row(
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

    fn checkbox_row(
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

    fn settings_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_text_input_control_with_align(
            input,
            value,
            placeholder,
            width,
            TextInputContentAlign::Start,
            cx,
        )
    }

    fn settings_text_input_control_with_align(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        align: TextInputContentAlign,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input_with_content_align(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
                align,
            )
            .w(px(width))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
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
}
