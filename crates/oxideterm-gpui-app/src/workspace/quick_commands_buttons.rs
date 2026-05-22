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
            idle_opacity: 1.0,
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..IconButtonOptions::compact(22.0)
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
            idle_opacity: 1.0,
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..IconButtonOptions::compact(18.0)
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
            size: 26.0,
            radius: ButtonRadius::Md,
            border: Some(rgb(tokens.ui.border)),
            idle_opacity: 1.0,
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            ..IconButtonOptions::compact(26.0)
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
