use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{
    SettingsInputDraftApply, ThemeEditorState, apply_persisted_settings_input_draft,
    editor_terminal_theme, editor_ui_colors, theme_tokens_for_system,
};

use super::{SettingsDraftEdit, SettingsInput};

pub(super) fn background_settings_for_controls(
    persisted: &PersistedSettings,
    editor: Option<&ThemeEditorState>,
    dark: bool,
) -> PersistedSettings {
    let mut settings = persisted.clone();
    let style = editor
        .map(|editor| editor.background.clone())
        .unwrap_or_else(|| persisted.terminal.background_for_scheme(Some(dark)));
    settings.terminal.apply_background_style(style);
    settings
}

pub fn prepare_draft_edit(
    settings: &PersistedSettings,
    input: SettingsInput,
    draft: &str,
    editor: Option<&ThemeEditorState>,
    dark: bool,
) -> SettingsDraftEdit {
    let mut next = if input.is_background() {
        background_settings_for_controls(settings, editor, dark)
    } else {
        settings.clone()
    };
    if matches!(input, SettingsInput::BackgroundEffectColor(_))
        && !draft.trim().is_empty()
        && let Some(effect) = &mut next.terminal.background_effect
    {
        let palette = if let Some(editor) = editor {
            [
                editor_ui_colors(&editor.ui_colors).accent,
                editor_terminal_theme(&editor.terminal_colors).cyan,
            ]
        } else {
            let tokens = theme_tokens_for_system(settings, dark);
            [tokens.ui.accent, tokens.terminal.cyan]
        };
        effect.colors.get_or_insert(palette);
    }
    match apply_persisted_settings_input_draft(&mut next, input, draft) {
        SettingsInputDraftApply::Applied if input.is_background() => {
            SettingsDraftEdit::Background(next.terminal.background_style())
        }
        SettingsInputDraftApply::Applied => SettingsDraftEdit::Settings(next),
        SettingsInputDraftApply::Invalid => SettingsDraftEdit::Invalid,
        SettingsInputDraftApply::Unhandled => SettingsDraftEdit::Unhandled,
    }
}
