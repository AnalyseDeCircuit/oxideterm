use super::*;

use oxideterm_gpui_ui::{
    ToolbarButtonOptions,
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant},
    modal::{dismissible_dialog_backdrop, overlay_content_boundary},
};

const LOCAL_SHELL_LAUNCHER_WIDTH: f32 = 480.0; // Match the Tauri shell launcher dialog width.
const LOCAL_SHELL_LAUNCHER_LIST_MAX_HEIGHT: f32 = 300.0; // Keep long shell lists inside the modal.

impl WorkspaceApp {
    pub(in crate::workspace) fn open_local_shell_launcher(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings();
        let shells = self.effective_local_shells_for_settings(settings);
        self.local_shell_launcher_selected_id = settings
            .local_terminal
            .default_shell_id
            .as_ref()
            .filter(|id| shells.iter().any(|shell| &shell.id == *id))
            .cloned()
            .or_else(|| shells.first().map(|shell| shell.id.clone()));
        self.local_shell_launcher_open = true;
        cx.notify();
    }

    fn close_local_shell_launcher(&mut self, cx: &mut Context<Self>) {
        self.local_shell_launcher_open = false;
        self.local_shell_launcher_selected_id = None;
        cx.notify();
    }

    pub(in crate::workspace) fn handle_local_shell_launcher_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.local_shell_launcher_open {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_local_shell_launcher(cx),
            "enter" => self.launch_selected_local_shell(window, cx),
            _ => {}
        }
        true
    }

    fn launch_selected_local_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected_id) = self.local_shell_launcher_selected_id.clone() else {
            return;
        };
        let settings = self.settings_store.settings();
        let Some(shell) = self
            .effective_local_shells_for_settings(settings)
            .into_iter()
            .find(|shell| shell.id == selected_id)
        else {
            return;
        };

        let mut terminal_config = self.local_terminal_config();
        terminal_config.shell = Some(shell.clone());
        self.edit_settings(
            |settings| {
                let recent = &mut settings.local_terminal.recent_shell_ids;
                recent.retain(|id| id != &shell.id);
                recent.insert(0, shell.id.clone());
                recent.truncate(5);
            },
            cx,
        );
        self.local_shell_launcher_open = false;
        self.local_shell_launcher_selected_id = None;
        let _ =
            self.create_local_terminal_tab_with_config(terminal_config, shell.label, window, cx);
    }

    pub(in crate::workspace) fn render_local_shell_launcher(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let settings = self.settings_store.settings();
        let default_shell_id = settings.local_terminal.default_shell_id.clone();
        let recent_shell_ids = settings.local_terminal.recent_shell_ids.clone();
        let selected_shell_id = self.local_shell_launcher_selected_id.clone();
        let mut shells = self.effective_local_shells_for_settings(settings);
        // Tauri sorts the configured default first, then recently launched shells.
        shells.sort_by_key(|shell| {
            if default_shell_id.as_deref() == Some(shell.id.as_str()) {
                (0, 0)
            } else if let Some(index) = recent_shell_ids.iter().position(|id| id == &shell.id) {
                (1, index)
            } else {
                (2, usize::MAX)
            }
        });

        let mut shell_list = div()
            .w_full()
            .max_h(px(LOCAL_SHELL_LAUNCHER_LIST_MAX_HEIGHT))
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap(px(4.0));
        for shell in shells {
            let shell_id = shell.id.clone();
            let selected = selected_shell_id.as_deref() == Some(shell.id.as_str());
            let is_default = default_shell_id.as_deref() == Some(shell.id.as_str());
            shell_list = shell_list.child(
                div()
                    .w_full()
                    .min_w_0()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x00)
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba((theme.bg_hover << 8) | 0x00)
                    })
                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        18.0,
                        rgb(if selected {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(shell.label)
                                    .when(is_default, |label| {
                                        label.child(self.text_badge(
                                            self.i18n.t("settings_view.local_terminal.default"),
                                            theme.warning,
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(shell.path.display().to_string()),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.local_shell_launcher_selected_id = Some(shell_id.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        let selected_is_default = selected_shell_id.as_deref() == default_shell_id.as_deref();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_local_shell_launcher(cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(
                div()
                    .w(px(LOCAL_SHELL_LAUNCHER_WIDTH))
                    .max_w_full()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("settings_view.local_terminal.select_shell")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.local_terminal.available_shells")),
                    )
                    .child(shell_list)
                    .child(
                        div()
                            .w_full()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .pt(px(12.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.ai.profile_set_default"),
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: selected_shell_id.is_none()
                                            || selected_is_default,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    let Some(selected_id) =
                                        this.local_shell_launcher_selected_id.clone()
                                    else {
                                        return;
                                    };
                                    this.edit_settings(
                                        |settings| {
                                            settings.local_terminal.default_shell_id =
                                                Some(selected_id.clone());
                                        },
                                        cx,
                                    );
                                    cx.notify();
                                }),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .child(self.workspace_toolbar_action_button(
                                        self.i18n.t("sessionManager.edit_properties.cancel"),
                                        None,
                                        ToolbarButtonOptions {
                                            button: ButtonOptions {
                                                variant: ButtonVariant::Secondary,
                                                size: ButtonSize::Sm,
                                                radius: ButtonRadius::Md,
                                                disabled: false,
                                            },
                                            ..ToolbarButtonOptions::default()
                                        },
                                        cx.listener(|this, _event, _window, cx| {
                                            this.close_local_shell_launcher(cx);
                                        }),
                                    ))
                                    .child(self.workspace_toolbar_action_button(
                                        self.i18n.t("workspace.new_local_terminal"),
                                        None,
                                        ToolbarButtonOptions {
                                            button: ButtonOptions {
                                                variant: ButtonVariant::Default,
                                                size: ButtonSize::Sm,
                                                radius: ButtonRadius::Md,
                                                disabled: selected_shell_id.is_none(),
                                            },
                                            ..ToolbarButtonOptions::default()
                                        },
                                        cx.listener(|this, _event, window, cx| {
                                            this.launch_selected_local_shell(window, cx);
                                        }),
                                    )),
                            ),
                    ),
            ))
            .into_any_element()
    }
}
