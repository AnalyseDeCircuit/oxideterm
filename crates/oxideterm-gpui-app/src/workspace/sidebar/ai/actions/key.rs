pub(in crate::workspace) use oxideterm_gpui_ai::state::sidebar_keys::AiChatPromptKeyAction;
use oxideterm_gpui_ai::state::sidebar_keys::{AiSidebarInput, AiSidebarKey};

pub(in crate::workspace) fn ai_chat_prompt_key_action(
    keystroke: &gpui::Keystroke,
    overrides: &serde_json::Map<String, serde_json::Value>,
    composing: bool,
    autocomplete: bool,
) -> Option<AiChatPromptKeyAction> {
    oxideterm_gpui_ai::state::sidebar_keys::ai_chat_prompt_key_action(
        keystroke,
        crate::keybindings::keystroke_matches_action(keystroke, "terminal.aiSubmit", overrides),
        composing,
        autocomplete,
    )
}

fn sidebar_input_target(input: AiSidebarInput) -> WorkspaceImeTarget {
    match input {
        AiSidebarInput::Composer => WorkspaceImeTarget::AiChatInput,
        AiSidebarInput::Rename => WorkspaceImeTarget::AiConversationRename,
        AiSidebarInput::MessageEdit => WorkspaceImeTarget::AiMessageEdit,
        AiSidebarInput::ModelSearch => WorkspaceImeTarget::AiModelSelectorSearch,
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_ai_sidebar_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = &event.keystroke;
        let autocomplete = if self.ai.state.read(cx).chat_ui().input_focused {
            self.ai_chat_autocomplete_items(cx)
        } else {
            Vec::new()
        };
        let composing = self
            .marked_text_for_target(WorkspaceImeTarget::AiChatInput, cx)
            .is_some();
        let submit = crate::keybindings::keystroke_matches_action(
            key,
            "terminal.aiSubmit",
            &self.settings_store.settings().keybindings.overrides,
        );
        let effect = self.ai.state.update(cx, |ai, cx| {
            ai.handle_sidebar_key(key, submit, composing, &autocomplete, cx)
        });
        match effect {
            AiSidebarKey::Pass => return false,
            AiSidebarKey::Handled => {}
            AiSidebarKey::ClearComposition => self.ime_marked_text = None,
            AiSidebarKey::BackspaceComposition(input) => {
                if self.ime_marked_text.take().is_some() {
                    self.notify_ime_target(sidebar_input_target(input), cx);
                }
            }
            AiSidebarKey::LiteralSpace(input) => {
                self.insert_ai_text_input_literal_space(sidebar_input_target(input), cx)
            }
            AiSidebarKey::Newline => {
                self.handle_active_text_input_newline(key, cx);
            }
            AiSidebarKey::Submit => self.send_ai_chat_draft(cx),
            AiSidebarKey::CloseModelPicker => {
                self.close_ai_model_selector(cx);
                cx.notify();
            }
            AiSidebarKey::MoveModelHighlight(delta) => {
                self.move_ai_model_selector_highlight(delta, cx);
                self.notify_ime_target(WorkspaceImeTarget::AiModelSelectorSearch, cx);
            }
            AiSidebarKey::ModelHighlightEdge(last) => {
                self.set_ai_model_selector_highlight_edge(last, cx);
                self.notify_ime_target(WorkspaceImeTarget::AiModelSelectorSearch, cx);
            }
            AiSidebarKey::SelectModel => {
                self.select_highlighted_ai_model(cx);
            }
            AiSidebarKey::CancelRename => {
                self.cancel_ai_conversation_rename(cx);
            }
            AiSidebarKey::SaveRename => {
                self.save_ai_conversation_rename(cx);
            }
            AiSidebarKey::CancelEdit => self.cancel_edit_ai_message(cx),
            AiSidebarKey::SaveEdit => self.save_ai_message_edit(cx),
        }
        true
    }

    pub(in crate::workspace) fn insert_ai_text_input_literal_space(
        &mut self,
        target: WorkspaceImeTarget,
        cx: &mut Context<Self>,
    ) {
        // Some GPUI platforms deliver Space without key_char, so write it
        // through the IME owner just like a browser textarea would.
        let replacement_range = self.ime_selection_range_for_target(target, cx);
        let caret = replacement_range
            .as_ref()
            .map(|range| range.start + " ".encode_utf16().count());
        self.replace_ime_target_text(target, replacement_range, " ", caret, cx);
        self.clear_ime_selection();
        if let Some(caret) = caret {
            self.set_ime_selection_from_anchor(target, caret, caret);
        }
    }
}

#[cfg(test)]
mod ai_chat_prompt_key_tests {
    use super::{AiChatPromptKeyAction::*, ai_chat_prompt_key_action};
    use crate::keybindings::{KeybindingSide, combo_from_keystroke, set_override};
    use gpui::Keystroke;
    use serde_json::Map;

    #[test]
    fn configured_send_key_controls_submit_and_newline() {
        for (binding, cases) in [
            (
                None,
                vec![
                    ("enter", Some(Submit)),
                    ("shift-enter", Some(Newline)),
                    ("ctrl-enter", None),
                ],
            ),
            (
                Some("ctrl-enter"),
                vec![
                    ("enter", Some(Newline)),
                    ("shift-enter", Some(Newline)),
                    ("ctrl-enter", Some(Submit)),
                ],
            ),
            (
                Some("cmd-enter"),
                vec![
                    ("enter", Some(Newline)),
                    ("cmd-enter", Some(Submit)),
                    ("ctrl-enter", None),
                ],
            ),
            (
                Some("shift-enter"),
                vec![("enter", Some(Newline)), ("shift-enter", Some(Submit))],
            ),
        ] {
            let mut overrides = Map::new();
            if let Some(binding) = binding {
                set_override(
                    &mut overrides,
                    "terminal.aiSubmit",
                    KeybindingSide::current(),
                    combo_from_keystroke(&Keystroke::parse(binding).unwrap()).unwrap(),
                );
            }
            for (key, expected) in cases {
                assert_eq!(
                    ai_chat_prompt_key_action(
                        &Keystroke::parse(key).unwrap(),
                        &overrides,
                        false,
                        false
                    ),
                    expected,
                    "{key}, {overrides:?}"
                );
            }
        }
    }

    #[test]
    fn composition_and_completion_do_not_accidentally_submit() {
        let mut overrides = Map::new();
        set_override(
            &mut overrides,
            "terminal.aiSubmit",
            KeybindingSide::current(),
            combo_from_keystroke(&Keystroke::parse("ctrl-enter").unwrap()).unwrap(),
        );
        for key in ["enter", "ctrl-enter", "shift-enter"] {
            assert_eq!(
                ai_chat_prompt_key_action(&Keystroke::parse(key).unwrap(), &overrides, true, true),
                None,
                "IME: {key}"
            );
        }
        for (key, expected) in [
            ("enter", Some(Complete)),
            ("tab", Some(Complete)),
            ("shift-enter", Some(Newline)),
            ("ctrl-enter", Some(Submit)),
        ] {
            assert_eq!(
                ai_chat_prompt_key_action(&Keystroke::parse(key).unwrap(), &overrides, false, true),
                expected,
                "completion: {key}"
            );
        }
    }
}
