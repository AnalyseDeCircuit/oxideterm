use super::*;

impl WorkspaceApp {
    pub(super) fn render_session_manager_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        // Tauri batch actions are normal shadcn Buttons. Keep the local icon
        // placement, but route activation through the shared toolbar guard.
        self.workspace_toolbar_action_button(
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
            listener,
        )
    }

    pub(super) fn render_toolbar_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
        has_background: bool,
        show_label: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let icon_color = match variant {
            ButtonVariant::Default => rgb(theme.bg),
            _ => rgb(theme.text),
        };
        // Toolbar commands match Tauri Button chrome while sharing the native
        // disabled/loading action guard with other workspace toolbars.
        self.workspace_toolbar_action_button(
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
            listener,
        )
    }

    pub(super) fn render_toolbar_link_icon(
        &self,
        icon: LucideIcon,
        label_key: &str,
        transfer_action: SessionTransferAction,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Link,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                show_label,
                icon_gap: Some(6.0),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgba(0x00000000)),
                // Tauri renders these import/export affordances as toolbar links,
                // not selectable labels. Reuse the shared button boundary while
                // preserving the old no-fill hover behavior.
                ..ToolbarButtonOptions::compact_text(
                    ButtonVariant::Link,
                    ButtonRadius::Md,
                    32.0,
                    0.0,
                    self.tokens.metrics.ui_text_sm,
                )
            },
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

    pub(in crate::workspace) fn render_session_text_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_session_text_input_with_options(target, value, placeholder, false, cx)
    }

    pub(super) fn render_session_text_input_with_options(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let workspace = cx.entity();
        let active = self.session_manager.focused_input == Some(target);
        let has_background = self.background_surface_active("session_manager");
        let marked = self
            .marked_text_for_target(WorkspaceImeTarget::SessionManager(target))
            .unwrap_or_default();
        let visually_empty = value.is_empty() && marked.is_empty();
        let text = if value.is_empty() && marked.is_empty() {
            placeholder
        } else if value.is_empty() {
            String::new()
        } else if secret {
            text_input_secret_mask(value)
        } else {
            value.to_string()
        };
        let marked_text = if secret {
            text_input_secret_mask(marked)
        } else {
            marked.to_string()
        };
        let input_target = WorkspaceImeTarget::SessionManager(target);
        let input_range = if active && !value.is_empty() && marked.is_empty() {
            self.ime_selected_range_for_target(input_target)
        } else {
            None
        }
        // Session-manager password fields keep the real value in IME state but
        // paint masked bullets, so selection/caret offsets need the shared
        // visual-range conversion before rendering.
        .map(|range| text_input_visual_range(value, secret, range));
        let selection_range = input_range.clone().filter(|range| range.start < range.end);
        let caret_offset = input_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let shows_selection = selection_range.is_some();
        let shows_positioned_caret = caret_offset.is_some() && !shows_selection;
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
                .text_color(if visually_empty {
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
                        .when(active && visually_empty, |input| {
                            input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                        })
                        .child(text_input_value_segments(
                            &self.tokens,
                            &text,
                            visually_empty,
                            selection_range,
                            caret_offset,
                            self.new_connection_caret_visible,
                        ))
                        .when(active && !marked_text.is_empty(), |input| {
                            input.child(
                                div()
                                    .underline()
                                    .text_color(rgb(theme.text))
                                    .child(marked_text),
                            )
                        })
                        .when(
                            active
                                && !visually_empty
                                && !shows_selection
                                && !shows_positioned_caret,
                            |input| {
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            },
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.session_manager.focused_input = Some(target);
                        this.ime_marked_text = None;
                        this.needs_active_pane_focus = false;
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(
                            WorkspaceImeTarget::SessionManager(target),
                            event,
                            window,
                            cx,
                        );
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

    pub(in crate::workspace) fn render_session_password_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_session_text_input_with_options(target, value, placeholder, true, cx)
    }
}
