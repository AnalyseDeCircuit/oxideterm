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
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(theme.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
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
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                has_background,
                show_label,
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
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
        let active = self.session_manager.read(cx).focused_input() == Some(target);
        let has_background = self.background_surface_active("session_manager");
        let marked = self
            .marked_text_for_target(WorkspaceImeTarget::SessionManager(target), cx)
            .unwrap_or_default();
        let visually_empty = value.is_empty() && marked.is_empty();
        let input_target = WorkspaceImeTarget::SessionManager(target);
        self.text_input_with_workspace_ime(
            input_target,
            (if target == SessionManagerInput::Search {
                self.sidebar_search_row(theme_bg(theme.bg, has_background))
            } else {
                div()
            })
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
            .when(target == SessionManagerInput::Search, |input| {
                input
                    .h(px(36.0))
                    .rounded(px(0.0))
                    .border_0()
                    .border_b_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        self.workspace_chrome_divider()
                    })
                    .bg(rgba(0x00000000))
            })
            .child(
                self.workspace_text_input(
                    input_target,
                    oxideterm_gpui_ui::text_input::TextInputView {
                        value,
                        placeholder,
                        focused: active,
                        caret_visible: self.input_caret.visible(),
                        secret,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(input_target, cx),
                        marked_text: Some(marked),
                    },
                )
                .flex_1()
                .min_w_0()
                .h_full()
                .border_0()
                .bg(rgba(0x00000000))
                .px(px(0.0)),
            ),
            move |this, cx| {
                this.session_manager.update(cx, |manager, cx| {
                    manager.focused_input = Some(target);
                    cx.notify();
                });
                this.needs_active_pane_focus = false;
            },
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_session_password_input(
        &self,
        target: SessionManagerInput,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let masked_value = {
            let manager = self.session_manager.read(cx);
            let value = match target {
                SessionManagerInput::OxideExportPassword => manager
                    .oxide_export_dialog
                    .as_ref()
                    .map(|dialog| dialog.password.as_str()),
                SessionManagerInput::OxideExportConfirmPassword => manager
                    .oxide_export_dialog
                    .as_ref()
                    .map(|dialog| dialog.confirm_password.as_str()),
                SessionManagerInput::OxideImportPassword => manager
                    .oxide_import_dialog
                    .as_ref()
                    .map(|dialog| dialog.password.as_str()),
                _ => None,
            }
            .unwrap_or_default();
            // Preserve UTF-16 offsets without copying the secret out of its owner.
            super::super::ime::secret_ime_proxy(value)
        };
        self.render_session_text_input_with_options(target, &masked_value, placeholder, true, cx)
    }
}
