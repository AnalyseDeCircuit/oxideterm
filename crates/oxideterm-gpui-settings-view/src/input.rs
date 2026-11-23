use gpui::{Context, KeyDownEvent};
use oxideterm_settings::{BackgroundStyle, PersistedSettings};
use oxideterm_settings_model::{
    SettingsInputDraftApply, apply_persisted_settings_input_draft, editor_terminal_theme,
    editor_ui_colors, theme_tokens_for_system,
};

use crate::{
    SettingsInput, entity::SettingsWorkspaceEntity, terminal_triggers::is_terminal_trigger_input,
};

pub enum SettingsInputKeyEffect {
    Ignored,
    Handled,
    CloseSearch,
    SubmitSearch,
    CloseTriggerScope,
    CloseTriggerEditor,
    CloseSemantic,
    CloseHighlight,
    CloseCommandSpecs,
    ClearIme,
}

pub enum SettingsDraftEdit {
    Settings(PersistedSettings),
    Background(BackgroundStyle),
    Invalid,
    Unhandled,
}

impl SettingsWorkspaceEntity {
    pub fn navigate_to_search_result(
        &mut self,
        result: &crate::SettingsSearchResult,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_settings_model::SettingsSectionId;
        match result.section {
            SettingsSectionId::AppearanceBackground => {
                self.background_effects_expanded = Some(true);
                self.background_interactions_expanded = Some(true);
            }
            SettingsSectionId::TerminalAwarenessTriggers => self.terminal_triggers.pane = None,
            _ => {}
        }
        self.set_active_tab(result.tab, cx);
        if let Some(page) = result.terminal_page {
            self.set_terminal_page(page, cx);
        }
        if let Some(page) = result.ai_page {
            self.set_ai_page(page, cx);
        }
        self.set_slider_drag(None);
        if self.settings_entity_focused_input() == Some(SettingsInput::SettingsSearch) {
            self.blur_settings_entity_input(cx);
        }
        cx.notify();
    }

    pub fn handle_settings_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> SettingsInputKeyEffect {
        use SettingsInputKeyEffect::*;
        let Some(input) = self.settings_entity_focused_input() else {
            return Ignored;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        match (key, input) {
            ("escape", SettingsInput::SettingsSearch) => CloseSearch,
            ("enter", SettingsInput::SettingsSearch) => SubmitSearch,
            ("escape", _) if is_terminal_trigger_input(input) => {
                if self.terminal_triggers.scope_picker_open {
                    CloseTriggerScope
                } else {
                    CloseTriggerEditor
                }
            }
            (
                "escape",
                SettingsInput::SemanticSchemeName
                | SettingsInput::SemanticSchemeRulePattern(_)
                | SettingsInput::SemanticSchemeRuleCapture(_)
                | SettingsInput::SemanticSchemeColor(_),
            ) => CloseSemantic,
            ("escape", _) if crate::highlight_page::is_highlight_input(input) => CloseHighlight,
            ("escape", SettingsInput::TerminalCommandSpecsJson) => CloseCommandSpecs,
            ("enter", _)
                if input == SettingsInput::TerminalCommandSpecsJson
                    || (is_terminal_trigger_input(input) && input.accepts_newline()) =>
            {
                self.replace_settings_entity_input(input, None, "\n", cx);
                Handled
            }
            ("escape" | "enter", _) => {
                self.blur_settings_entity_input(cx);
                ClearIme
            }
            ("backspace" | "delete", _) if !modifiers.platform && !modifiers.control => {
                self.pop_settings_entity_input(input, cx);
                Handled
            }
            _ => Handled,
        }
    }

    pub fn background_settings_for_controls(
        &self,
        persisted: &PersistedSettings,
    ) -> PersistedSettings {
        let mut settings = persisted.clone();
        let style = self
            .theme_editor()
            .map(|editor| editor.background.clone())
            .unwrap_or_else(|| {
                persisted
                    .terminal
                    .background_for_scheme(Some(self.appearance_edit_dark.unwrap_or(false)))
            });
        settings.terminal.apply_background_style(style);
        settings
    }

    pub fn prepare_draft_edit(
        &self,
        settings: &PersistedSettings,
        input: SettingsInput,
        draft: &str,
    ) -> SettingsDraftEdit {
        let mut next = if input.is_background() {
            self.background_settings_for_controls(settings)
        } else {
            settings.clone()
        };
        if matches!(input, SettingsInput::BackgroundEffectColor(_))
            && !draft.trim().is_empty()
            && let Some(effect) = &mut next.terminal.background_effect
        {
            let palette = if let Some(editor) = self.theme_editor() {
                [
                    editor_ui_colors(&editor.ui_colors).accent,
                    editor_terminal_theme(&editor.terminal_colors).cyan,
                ]
            } else {
                let tokens =
                    theme_tokens_for_system(settings, self.appearance_edit_dark.unwrap_or(false));
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
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};
    use oxideterm_i18n::{I18n, Locale};
    use oxideterm_settings_model::{SettingsSectionId, SettingsTab, TerminalSettingsPage};

    use super::*;

    #[gpui::test]
    fn settings_search_keeps_query_across_result_navigation(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            let input = SettingsInput::SettingsSearch;
            settings.set_terminal_page(TerminalSettingsPage::Input, cx);
            settings.open_settings_search(cx);
            assert!(settings.focus_settings_entity_input(input, cx));
            assert!(settings.replace_settings_entity_input(input, None, "font", cx));

            let results = crate::settings_search_results(&I18n::new(Locale::En), "font");
            for (section, tab) in [
                (
                    SettingsSectionId::TerminalDisplayFont,
                    SettingsTab::Terminal,
                ),
                (SettingsSectionId::IdeTypography, SettingsTab::Ide),
            ] {
                let result = results
                    .iter()
                    .find(|result| result.section == section)
                    .unwrap();
                settings.navigate_to_search_result(result, cx);
                assert_eq!(settings.route_snapshot().active_tab, tab);
                assert!(settings.settings_search_open());
                assert_eq!(settings.settings_search_query(), "font");
                assert_eq!(settings.settings_entity_focused_input(), None);
            }
            assert_eq!(
                settings.route_snapshot().terminal_page,
                TerminalSettingsPage::Display
            );
        });
    }
}
