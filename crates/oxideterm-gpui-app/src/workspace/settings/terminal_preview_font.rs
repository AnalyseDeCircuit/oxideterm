use super::*;

impl WorkspaceApp {
    pub(super) fn settings_terminal_preview_font(
        settings: &PersistedSettings,
        cx: &App,
    ) -> oxideterm_gpui_settings_view::components::TerminalPreviewFont {
        let family = settings
            .terminal
            .font_family
            .terminal_family_name(&settings.terminal.custom_font_family);
        let cjk_family = settings.terminal.cjk_font_family.trim();
        let cjk_family = (!cjk_family.is_empty()).then_some(cjk_family);
        let resolved = oxideterm_gpui_terminal::terminal_ui::resolve_terminal_font(
            cx.text_system(),
            &family,
            cjk_family,
            settings.terminal.font_ligatures,
            settings.terminal.font_weight as f32,
        );
        let preview_line_height =
            oxideterm_gpui_terminal::terminal_ui::terminal_line_height_for_font(
                cx.text_system(),
                &resolved.font,
                px(settings.terminal.font_size as f32),
                settings.terminal.line_height as f32,
            );
        let preview_letter_spacing =
            oxideterm_gpui_terminal::terminal_ui::terminal_letter_spacing_for_font(
                cx.text_system(),
                &resolved.font,
                px(settings.terminal.font_size as f32),
                settings.terminal.character_spacing as f32,
            );
        oxideterm_gpui_settings_view::components::TerminalPreviewFont {
            font: resolved.font,
            unavailable_families: resolved.unavailable_families,
            font_size: px(settings.terminal.font_size as f32),
            line_height: preview_line_height,
            letter_spacing: preview_letter_spacing,
        }
    }
}
