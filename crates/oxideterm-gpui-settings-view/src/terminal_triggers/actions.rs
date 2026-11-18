use gpui::Context;

use crate::entity::SettingsWorkspaceEntity;

use super::TerminalTriggerRequest;

pub struct TerminalTriggerOutcome {
    pub clear_ime: bool,
    pub refresh_runtime: bool,
    pub shell_trust: Option<bool>,
}

impl TerminalTriggerRequest {
    pub fn opens_editor(&self) -> bool {
        matches!(
            self,
            Self::OpenNew | Self::OpenEdit(_) | Self::OpenDuplicate(_, _)
        )
    }
}

impl SettingsWorkspaceEntity {
    pub fn apply_terminal_trigger_request(
        &mut self,
        request: TerminalTriggerRequest,
        shell_enabled: bool,
        cx: &mut Context<Self>,
    ) -> TerminalTriggerOutcome {
        use TerminalTriggerRequest::*;
        let clear_ime = request.opens_editor()
            || matches!(
                request,
                CloseEditor
                    | CloseScopePicker
                    | EditDraft {
                        clear_input: true,
                        ..
                    }
            );
        if clear_ime {
            self.blur_terminal_trigger_input(cx);
        }
        let mut outcome = TerminalTriggerOutcome {
            clear_ime,
            refresh_runtime: false,
            shell_trust: None,
        };
        if matches!(request, SaveEditor) {
            if !self.apply_active_trigger_input() {
                self.terminal_triggers.error_key =
                    Some("settings_view.terminal.triggers.validation_failed");
            } else if self.terminal_triggers.save_editor(shell_enabled) {
                self.blur_terminal_trigger_input(cx);
                outcome.clear_ime = true;
                outcome.refresh_runtime = true;
            }
        } else {
            let state = &mut self.terminal_triggers;
            match request {
                OpenNew => state.new_rule(),
                OpenEdit(id) => state.edit_rule(&id),
                OpenDuplicate(id, suffix) => state.duplicate_rule(&id, &suffix),
                CloseEditor => state.cancel_edit(),
                CloseScopePicker => state.scope_picker_open = false,
                ShowGlobal => state.pane = None,
                ToggleShellTrust(true) => outcome.shell_trust = Some(false),
                ToggleShellTrust(false) => state.shell_confirmation_pending = true,
                CancelShellTrust => state.shell_confirmation_pending = false,
                ConfirmShellTrust => outcome.shell_trust = Some(true),
                ToggleRule(id) => outcome.refresh_runtime = state.toggle_rule(&id),
                DeleteRule(id) => outcome.refresh_runtime = state.delete_rule(&id),
                EditDraft { edit, .. } => edit(state),
                SaveEditor | ToggleForPane(_, _) => {}
            }
        }
        cx.notify();
        outcome
    }

    pub fn blur_terminal_trigger_input(&mut self, cx: &mut Context<Self>) -> bool {
        if self
            .settings_entity_focused_input()
            .is_some_and(super::is_terminal_trigger_input)
        {
            self.blur_settings_entity_input(cx);
            true
        } else {
            false
        }
    }
}
