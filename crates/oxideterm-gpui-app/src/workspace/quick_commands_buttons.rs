use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions, ToolbarButtonOptions,
};

impl WorkspaceApp {
    fn quick_command_icon_button(
        &self,
        icon: LucideIcon,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Quick command chrome mirrors Tauri icon buttons, but activation must
        // share the workspace guard so disabled/loading behavior stays browser-like.
        self.workspace_icon_action_button(
            icon,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Sm)
            },
            listener,
            cx,
        )
    }

    fn quick_command_mini_button(
        &self,
        icon: LucideIcon,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_icon_action_button(
            icon,
            12.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(18.0, ButtonRadius::Sm)
            },
            listener,
            cx,
        )
    }

    fn quick_command_action_button(
        &self,
        icon: LucideIcon,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_icon_action_button(
            icon,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                border: Some(rgb(self.tokens.ui.border)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(26.0, ButtonRadius::Md)
            },
            listener,
            cx,
        )
    }

    fn quick_command_text_button(
        &self,
        label: String,
        enabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        // Quick command editor actions are visually outline buttons in Tauri
        // even when disabled. Use the shared action wrapper so disabled rows no
        // longer keep local mouse handlers attached.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                height: Some(28.0),
                padding_x: Some(10.0),
                text_color: Some(if enabled {
                    rgb(self.tokens.ui.text)
                } else {
                    rgba((self.tokens.ui.text_muted << 8) | 0x80)
                }),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }
}
