use crate::SettingsKeybindingScopeFilter;
use gpui::{Context, KeyDownEvent};
use oxideterm_gpui_ui::confirm::{self as browser_behavior, ConfirmDialogAction};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeybindingRecordingFooterAction {
    Confirm,
    Cancel,
}

const KEYBINDING_RECORDING_FOOTER_ACTIONS: [KeybindingRecordingFooterAction; 2] = [
    KeybindingRecordingFooterAction::Confirm,
    KeybindingRecordingFooterAction::Cancel,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeybindingRecordingKeyAction {
    Confirm,
    Handled,
}

/// Transfers the completed recording into the persistence/keymap adapter without cloning it.
pub struct KeybindingRecordingCommit {
    pub action_id: String,
    pub combo: crate::keybindings::KeyCombo,
}

#[derive(Clone, Copy)]
pub struct KeybindingResetConfirmSnapshot {
    pub phase: oxideterm_gpui_ui::motion::ExitPhase,
    pub focused_action: Option<ConfirmDialogAction>,
}

pub struct KeybindingState {
    keybinding_scope_filter: SettingsKeybindingScopeFilter,
    pub(crate) keybinding_search_query: String,
    keybinding_recording_action_id: Option<String>,
    keybinding_conflict_action_ids: Vec<String>,
    keybinding_recording_combo: Option<crate::keybindings::KeyCombo>,
    keybinding_recording_footer_focus: Option<KeybindingRecordingFooterAction>,
}

impl Default for KeybindingState {
    fn default() -> Self {
        Self {
            keybinding_scope_filter: SettingsKeybindingScopeFilter::All,
            keybinding_search_query: String::new(),
            keybinding_recording_action_id: None,
            keybinding_conflict_action_ids: Vec::new(),
            keybinding_recording_combo: None,
            keybinding_recording_footer_focus: None,
        }
    }
}
impl KeybindingState {
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn clear_recording(&mut self) {
        self.keybinding_recording_action_id = None;
        self.keybinding_conflict_action_ids.clear();
        self.keybinding_recording_combo = None;
        self.keybinding_recording_footer_focus = None;
    }

    pub fn keybinding_scope_filter(&self) -> SettingsKeybindingScopeFilter {
        self.keybinding_scope_filter
    }

    pub fn set_keybinding_scope_filter<T: 'static>(
        &mut self,
        filter: SettingsKeybindingScopeFilter,
        cx: &mut Context<T>,
    ) -> bool {
        if self.keybinding_scope_filter == filter {
            return false;
        }
        self.keybinding_scope_filter = filter;
        cx.notify();
        true
    }

    pub fn keybinding_search_query(&self) -> &str {
        &self.keybinding_search_query
    }

    pub fn keybinding_recording_action_id(&self) -> Option<&str> {
        self.keybinding_recording_action_id.as_deref()
    }

    pub fn keybinding_recording_combo(&self) -> Option<&crate::keybindings::KeyCombo> {
        self.keybinding_recording_combo.as_ref()
    }

    pub fn keybinding_conflicts(&self) -> &[String] {
        &self.keybinding_conflict_action_ids
    }

    pub fn keybinding_recording_footer_focus(&self) -> Option<KeybindingRecordingFooterAction> {
        self.keybinding_recording_footer_focus
    }

    pub fn start_keybinding_recording<T: 'static>(
        &mut self,
        action_id: impl Into<String>,
        cx: &mut Context<T>,
    ) {
        self.keybinding_recording_action_id = Some(action_id.into());
        self.keybinding_conflict_action_ids.clear();
        self.keybinding_recording_combo = None;
        self.keybinding_recording_footer_focus = None;
        cx.notify();
    }

    pub fn stop_keybinding_recording<T: 'static>(&mut self, cx: &mut Context<T>) {
        let changed = self.keybinding_recording_action_id.take().is_some()
            || !self.keybinding_conflict_action_ids.is_empty()
            || self.keybinding_recording_combo.take().is_some()
            || self.keybinding_recording_footer_focus.take().is_some();
        self.keybinding_conflict_action_ids.clear();
        if changed {
            cx.notify();
        }
    }

    pub fn handle_keybinding_recording_key<T: 'static>(
        &mut self,
        event: &KeyDownEvent,
        overrides: &serde_json::Map<String, serde_json::Value>,
        definitions: &[crate::keybindings::ActionDefinition],
        cx: &mut Context<T>,
    ) -> Option<KeybindingRecordingKeyAction> {
        if self.keybinding_recording_action_id.is_none() {
            return None;
        }
        if event.keystroke.key.as_str() == "escape"
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
        {
            self.stop_keybinding_recording(cx);
            return Some(KeybindingRecordingKeyAction::Handled);
        }

        if self.keybinding_recording_combo.is_some()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            match browser_behavior::modal_footer_key_action(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                &KEYBINDING_RECORDING_FOOTER_ACTIONS,
                self.keybinding_recording_footer_focus,
                KeybindingRecordingFooterAction::Confirm,
            ) {
                Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                    self.stop_keybinding_recording(cx);
                    return Some(KeybindingRecordingKeyAction::Handled);
                }
                Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                    // Native captures keydown globally, so the Entity mirrors
                    // the browser footer focus contract after a combo exists.
                    self.keybinding_recording_footer_focus = Some(action);
                    cx.notify();
                    return Some(KeybindingRecordingKeyAction::Handled);
                }
                Some(browser_behavior::ModalFooterKeyAction::Activate(
                    KeybindingRecordingFooterAction::Confirm,
                )) => {
                    self.keybinding_recording_footer_focus = None;
                    return Some(KeybindingRecordingKeyAction::Confirm);
                }
                Some(browser_behavior::ModalFooterKeyAction::Activate(
                    KeybindingRecordingFooterAction::Cancel,
                )) => {
                    self.stop_keybinding_recording(cx);
                    return Some(KeybindingRecordingKeyAction::Handled);
                }
                None => {}
            }
        }

        let action_id = self
            .keybinding_recording_action_id
            .as_deref()
            .expect("recording presence checked above");
        let combo = crate::keybindings::combo_from_keystroke(&event.keystroke)?;
        let side = crate::keybindings::KeybindingSide::current();
        self.keybinding_conflict_action_ids = crate::keybindings::conflicts_in_definitions(
            action_id,
            &combo,
            overrides,
            side,
            definitions,
        )
        .into_iter()
        .map(|definition| definition.id.to_string())
        .collect();
        self.keybinding_recording_combo = Some(combo);
        self.keybinding_recording_footer_focus = None;
        cx.notify();
        Some(KeybindingRecordingKeyAction::Handled)
    }

    pub fn activate_keybinding_recording_footer<T: 'static>(
        &mut self,
        action: KeybindingRecordingFooterAction,
        cx: &mut Context<T>,
    ) -> bool {
        self.keybinding_recording_footer_focus = None;
        match action {
            KeybindingRecordingFooterAction::Confirm => true,
            KeybindingRecordingFooterAction::Cancel => {
                self.stop_keybinding_recording(cx);
                false
            }
        }
    }

    pub fn take_keybinding_recording_commit<T: 'static>(
        &mut self,
        cx: &mut Context<T>,
    ) -> Option<KeybindingRecordingCommit> {
        let action_id = self.keybinding_recording_action_id.take()?;
        let Some(combo) = self.keybinding_recording_combo.take() else {
            self.keybinding_recording_action_id = Some(action_id);
            return None;
        };
        self.keybinding_conflict_action_ids.clear();
        self.keybinding_recording_footer_focus = None;
        cx.notify();
        Some(KeybindingRecordingCommit { action_id, combo })
    }
}
