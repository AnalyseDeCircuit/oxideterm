impl WorkspaceApp {
    fn render_session_manager_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        toolbar_button(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(theme.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_position: ToolbarButtonIconPosition::Trailing,
                ..ToolbarButtonOptions::default()
            },
        )
    }

    fn render_toolbar_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
        has_background: bool,
        show_label: bool,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let icon_color = match variant {
            ButtonVariant::Default => rgb(theme.bg),
            _ => rgb(theme.text),
        };
        toolbar_button(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(icon, 16.0, icon_color)),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                has_background,
                show_label,
                ..ToolbarButtonOptions::default()
            },
        )
    }

    fn render_toolbar_link_icon(
        &self,
        icon: LucideIcon,
        label_key: &str,
        transfer_action: SessionTransferAction,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(self.tokens.ui.text),
            ))
            .when(show_label, |button| button.child(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    match transfer_action {
                        SessionTransferAction::ImportOxide => this.open_oxide_import_dialog(cx),
                        SessionTransferAction::ExportOxide => this.open_oxide_export_dialog(cx),
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_session_text_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let workspace = cx.entity();
        let active = self.session_manager.focused_input == Some(target);
        let has_background = self
            .terminal_background_preferences("session_manager")
            .is_some();
        let text = if value.is_empty() {
            placeholder
        } else {
            value.to_string()
        };
        let input_target = WorkspaceImeTarget::SessionManager(target);
        text_input_anchor_probe(
            input_target.anchor_id(),
            div()
                .h(px(32.0))
                .w_full()
                .px_3()
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if active {
                    rgb(theme.accent)
                } else {
                    theme_border_half(theme.border, has_background)
                })
                .bg(theme_input_bg(theme.bg, has_background))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(if value.is_empty() {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                })
                .when(
                    matches!(
                        target,
                        SessionManagerInput::Search | SessionManagerInput::SavedSearch
                    ),
                    |input| {
                        input.child(Self::render_lucide_icon(
                            LucideIcon::Search,
                            16.0,
                            rgb(theme.text_muted),
                        ))
                    },
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .when(active && value.is_empty(), |input| {
                            input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                        })
                        .child(div().truncate().child(text))
                        .when_some(
                            self.marked_text_for_target(input_target),
                            |input, marked| {
                                input.child(
                                    div()
                                        .underline()
                                        .text_color(rgb(theme.text))
                                        .child(marked.to_string()),
                                )
                            },
                        )
                        .when(active && !value.is_empty(), |input| {
                            input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                        }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.session_manager.focused_input = Some(target);
                        this.ime_marked_text = None;
                        this.needs_active_pane_focus = false;
                        window.focus(&this.focus_handle);
                        this.begin_ime_selection_from_mouse_down(WorkspaceImeTarget::SessionManager(target), event, window, cx);
                        cx.notify();
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

    pub(super) fn render_session_password_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let masked = if value.is_empty() {
            String::new()
        } else {
            "•".repeat(value.chars().count())
        };
        self.render_session_text_input(target, &masked, placeholder, cx)
    }
}
