fn quick_command_icon_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .cursor_pointer()
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let theme = tokens.ui;
            move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            14.0,
            rgb(tokens.ui.text_muted),
        ))
}

fn quick_command_mini_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .cursor_pointer()
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let theme = tokens.ui;
            move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.accent))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            12.0,
            rgb(tokens.ui.text_muted),
        ))
}

fn quick_command_action_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .cursor_pointer()
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let theme = tokens.ui;
            move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            14.0,
            rgb(tokens.ui.text_muted),
        ))
}

fn quick_command_text_button(tokens: &ThemeTokens, label: String, enabled: bool) -> gpui::Div {
    div()
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .text_color(if enabled {
            rgb(tokens.ui.text)
        } else {
            rgba((tokens.ui.text_muted << 8) | 0x80)
        })
        .when(enabled, |button| {
            let theme = tokens.ui;
            button
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.bg_hover)))
        })
        .child(label)
}
