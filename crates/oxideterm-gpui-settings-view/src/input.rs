#[cfg(not(target_family = "wasm"))]
use gpui::{Context, KeyDownEvent};
use oxideterm_settings::{BackgroundStyle, PersistedSettings};

use crate::SettingsInput;
#[cfg(not(target_family = "wasm"))]
use crate::{entity::SettingsWorkspaceEntity, terminal_triggers::is_terminal_trigger_input};

mod draft;
#[cfg(target_family = "wasm")]
pub use draft::prepare_draft_edit;

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

#[cfg(not(target_family = "wasm"))]
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
        draft::background_settings_for_controls(
            persisted,
            self.theme_editor(),
            self.appearance_edit_dark.unwrap_or(false),
        )
    }

    pub fn prepare_draft_edit(
        &self,
        settings: &PersistedSettings,
        input: SettingsInput,
        draft: &str,
    ) -> SettingsDraftEdit {
        draft::prepare_draft_edit(
            settings,
            input,
            draft,
            self.theme_editor(),
            self.appearance_edit_dark.unwrap_or(false),
        )
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
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
