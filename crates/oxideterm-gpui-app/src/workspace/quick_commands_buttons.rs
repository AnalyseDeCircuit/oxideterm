use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
    ToolbarButtonOptions, icon_button, toolbar_button,
};

fn quick_command_icon_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    icon_button(
        tokens,
        WorkspaceApp::render_lucide_icon(
            icon,
            14.0,
            rgb(tokens.ui.text_muted),
        ),
        IconButtonOptions {
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Sm)
        },
    )
}

fn quick_command_mini_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    icon_button(
        tokens,
        WorkspaceApp::render_lucide_icon(
            icon,
            12.0,
            rgb(tokens.ui.text_muted),
        ),
        IconButtonOptions {
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..IconButtonOptions::opaque_toolbar(18.0, ButtonRadius::Sm)
        },
    )
}

fn quick_command_action_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    icon_button(
        tokens,
        WorkspaceApp::render_lucide_icon(
            icon,
            14.0,
            rgb(tokens.ui.text_muted),
        ),
        IconButtonOptions {
            border: Some(rgb(tokens.ui.border)),
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..IconButtonOptions::opaque_toolbar(26.0, ButtonRadius::Md)
        },
    )
}

fn quick_command_text_button(tokens: &ThemeTokens, label: String, enabled: bool) -> gpui::Div {
    // Quick command editor actions are visually outline buttons in Tauri even
    // when disabled. Use the shared toolbar primitive so disabled cursor,
    // loading, and future focus-visible behavior do not diverge per feature.
    toolbar_button(
        tokens,
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
                rgb(tokens.ui.text)
            } else {
                rgba((tokens.ui.text_muted << 8) | 0x80)
            }),
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..ToolbarButtonOptions::default()
        },
    )
}
