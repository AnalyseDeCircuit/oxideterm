use gpui::{Context, Keystroke};
use oxideterm_ai::AiAutocompleteCandidate;
use oxideterm_gpui_ui::confirm::{InlineFooterInputKeyAction, inline_footer_input_key_action};

use super::{AiChatFooterAction, AiWorkspaceEntity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiChatPromptKeyAction {
    Submit,
    Newline,
    Complete,
}

pub fn ai_chat_prompt_key_action(
    keystroke: &gpui::Keystroke,
    submit_binding: bool,
    composing: bool,
    autocomplete: bool,
) -> Option<AiChatPromptKeyAction> {
    if composing {
        return None;
    }
    let modifiers = keystroke.modifiers;
    let plain = !modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.shift;
    // Unmodified Enter/Tab accept a visible completion before the composer can submit.
    if autocomplete && plain && matches!(keystroke.key.as_str(), "enter" | "tab") {
        return Some(AiChatPromptKeyAction::Complete);
    }
    if submit_binding {
        return Some(AiChatPromptKeyAction::Submit);
    }
    if keystroke.key == "enter" && !modifiers.platform && !modifiers.control && !modifiers.alt {
        return Some(AiChatPromptKeyAction::Newline);
    }
    None
}

#[derive(Clone, Copy)]
pub enum AiSidebarInput {
    Composer,
    Rename,
    MessageEdit,
    ModelSearch,
}

pub enum AiSidebarKey {
    Pass,
    Handled,
    ClearComposition,
    BackspaceComposition(AiSidebarInput),
    LiteralSpace(AiSidebarInput),
    Newline,
    Submit,
    CloseModelPicker,
    MoveModelHighlight(isize),
    ModelHighlightEdge(bool),
    SelectModel,
    CancelRename,
    SaveRename,
    CancelEdit,
    SaveEdit,
}

impl AiWorkspaceEntity {
    pub fn handle_sidebar_key(
        &mut self,
        keystroke: &Keystroke,
        submit_binding: bool,
        composing: bool,
        autocomplete: &[AiAutocompleteCandidate],
        cx: &mut Context<Self>,
    ) -> AiSidebarKey {
        use AiSidebarInput::*;
        use AiSidebarKey::*;
        let m = keystroke.modifiers;
        let key = keystroke.key.as_str();
        let literal_space = matches!(key, "space" | " ")
            && ai_text_input_space_inserts_literal(m.platform, m.control, m.alt);

        if let Some(selection) = self.chat_ui().tool_candidate_selection.clone() {
            if m.platform {
                return Pass;
            }
            match key {
                "down" | "arrowdown" => {
                    self.move_tool_candidate_selection(1);
                }
                "up" | "arrowup" => {
                    self.move_tool_candidate_selection(-1);
                }
                "home" => {
                    self.move_tool_candidate_selection(-(selection.candidate_count as isize));
                }
                "end" => {
                    self.move_tool_candidate_selection(selection.candidate_count as isize);
                }
                "enter" | "escape" => {
                    self.resolve_tool_candidate_selection(
                        selection.generation,
                        &selection.tool_call_id,
                        (key == "enter").then_some(selection.selected_index),
                    );
                }
                _ => return Handled,
            }
            cx.notify();
            return Handled;
        }

        if self.model_selector_open() && self.model_selector_search_focused() {
            if m.platform {
                return Pass;
            }
            return match key {
                "escape" | "tab" => CloseModelPicker,
                "down" | "arrowdown" => MoveModelHighlight(1),
                "up" | "arrowup" => MoveModelHighlight(-1),
                "home" => ModelHighlightEdge(false),
                "end" => ModelHighlightEdge(true),
                "enter" => SelectModel,
                "backspace" => key_backspace(self.pop_model_selector_search(), ModelSearch, cx),
                _ => Handled,
            };
        }

        if self.chat_ui().renaming_conversation_id.is_some()
            && self.chat_ui().renaming_conversation_focused
        {
            if m.platform {
                return Pass;
            }
            return match key {
                "escape" => CancelRename,
                "enter" | "tab" => SaveRename,
                "backspace" => key_backspace(self.pop_conversation_rename(), Rename, cx),
                _ if literal_space => LiteralSpace(Rename),
                _ => Handled,
            };
        }

        if self.chat_ui().editing_message_id.is_some() && self.chat_ui().editing_message_focused {
            if m.platform {
                return Pass;
            }
            return match key {
                "escape" => CancelEdit,
                "backspace" => key_backspace(self.pop_message_edit(), MessageEdit, cx),
                "enter" if !m.shift => SaveEdit,
                "enter" | "tab" => {
                    if key == "enter" {
                        self.push_message_edit_newline();
                    } else {
                        self.blur_message_edit();
                    }
                    cx.notify();
                    ClearComposition
                }
                _ if literal_space => LiteralSpace(MessageEdit),
                _ => Handled,
            };
        }

        let footer_actions = &[AiChatFooterAction::Submit];
        if let Some(action) = self.chat_ui().footer_focus {
            if m.platform {
                return Pass;
            }
            return inline_footer_input_key_action(
                key,
                m.shift,
                footer_actions,
                false,
                Some(action),
                AiChatFooterAction::Submit,
            )
            .map_or(Handled, |action| self.apply_footer_key(action, cx));
        }

        if !self.chat_ui().input_focused {
            return Pass;
        }
        let prompt = ai_chat_prompt_key_action(
            keystroke,
            submit_binding,
            composing,
            !autocomplete.is_empty(),
        );
        if m.platform && prompt.is_none() {
            return Pass;
        }
        match prompt {
            Some(AiChatPromptKeyAction::Complete) => {
                let index = self
                    .chat_ui()
                    .autocomplete_index
                    .min(autocomplete.len() - 1);
                self.apply_chat_autocomplete(&autocomplete[index]);
                cx.notify();
                return ClearComposition;
            }
            Some(AiChatPromptKeyAction::Submit) => return Submit,
            Some(AiChatPromptKeyAction::Newline) => return Newline,
            None => {}
        }

        if !autocomplete.is_empty() {
            match key {
                "down" | "arrowdown" | "up" | "arrowup" => {
                    self.move_chat_autocomplete(
                        if matches!(key, "down" | "arrowdown") {
                            1
                        } else {
                            -1
                        },
                        autocomplete.len(),
                    );
                    cx.notify();
                    return Handled;
                }
                "escape" => {
                    self.suppress_chat_autocomplete();
                    cx.notify();
                    return ClearComposition;
                }
                _ => {}
            }
        }
        let available = if self.chat_is_loading() || !self.chat_ui().draft.trim().is_empty() {
            &footer_actions[..]
        } else {
            &[]
        };
        if let Some(action) = inline_footer_input_key_action(
            key,
            m.shift,
            available,
            true,
            None,
            AiChatFooterAction::Submit,
        ) {
            return self.apply_footer_key(action, cx);
        }
        match key {
            "backspace" => key_backspace(self.pop_chat_draft(), Composer, cx),
            _ if literal_space => LiteralSpace(Composer),
            _ => Handled,
        }
    }

    fn apply_footer_key(
        &mut self,
        action: InlineFooterInputKeyAction<AiChatFooterAction>,
        cx: &mut Context<Self>,
    ) -> AiSidebarKey {
        match action {
            InlineFooterInputKeyAction::ClearFocus => self.blur_chat_input(false),
            InlineFooterInputKeyAction::FocusInput => self.focus_chat_input(),
            InlineFooterInputKeyAction::FocusFooter(action) => {
                self.set_chat_footer_focus(Some(action))
            }
            InlineFooterInputKeyAction::Activate(AiChatFooterAction::Submit) => {
                if !self.chat_ui().draft.trim().is_empty() {
                    return AiSidebarKey::Submit;
                }
                self.clear_chat_footer_focus();
                cx.notify();
                return AiSidebarKey::Handled;
            }
        };
        cx.notify();
        AiSidebarKey::ClearComposition
    }
}

fn key_backspace(
    changed: bool,
    input: AiSidebarInput,
    cx: &mut Context<AiWorkspaceEntity>,
) -> AiSidebarKey {
    if changed {
        cx.notify();
        AiSidebarKey::Handled
    } else {
        AiSidebarKey::BackspaceComposition(input)
    }
}

fn ai_text_input_space_inserts_literal(platform: bool, control: bool, alt: bool) -> bool {
    !platform && !control && !alt
}

#[cfg(test)]
mod tests {
    use super::ai_text_input_space_inserts_literal;

    #[test]
    fn ai_text_input_plain_space_inserts_literal() {
        assert!(ai_text_input_space_inserts_literal(false, false, false));
    }

    #[test]
    fn ai_text_input_modified_space_falls_through() {
        assert!(!ai_text_input_space_inserts_literal(true, false, false));
        assert!(!ai_text_input_space_inserts_literal(false, true, false));
        assert!(!ai_text_input_space_inserts_literal(false, false, true));
    }
}
