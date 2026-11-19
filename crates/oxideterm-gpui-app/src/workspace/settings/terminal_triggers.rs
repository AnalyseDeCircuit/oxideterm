use super::*;
use oxideterm_gpui_settings_view::terminal_triggers::TerminalTriggerRequest as TriggerRequest;
pub(in crate::workspace) use oxideterm_gpui_settings_view::terminal_triggers::TerminalTriggersSettingsState;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_terminal_trigger_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_terminal_trigger(false, |state| state.pane = None, cx);
        self.open_terminal_trigger_settings_page(window, cx);
    }

    pub(in crate::workspace) fn open_terminal_trigger_settings_for_pane(
        &mut self,
        pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_terminal_trigger(false, |state| state.pane = Some(pane_id), cx);
        self.open_terminal_trigger_settings_page(window, cx);
    }

    fn open_terminal_trigger_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Terminal, cx);
            settings.set_terminal_page(TerminalSettingsPage::Awareness, cx);
        });
        self.sync_settings_section_list_state(cx);
        self.settings_workspace
            .read(cx)
            .section_list
            .scroll_to_section(
                SettingsTab::Terminal,
                SettingsSectionId::TerminalAwarenessTriggers,
                self.settings_dynamic_section_counts(cx),
            );
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn clear_terminal_trigger_input_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings_workspace
            .update(cx, |settings, cx| settings.blur_terminal_trigger_input(cx))
        {
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace) fn edit_terminal_trigger<R>(
        &mut self,
        clear_input: bool,
        edit: impl FnOnce(&mut TerminalTriggersSettingsState) -> R,
        cx: &mut Context<Self>,
    ) -> R {
        if clear_input {
            self.clear_terminal_trigger_input_focus(cx);
        }
        self.settings_workspace.update(cx, |settings, cx| {
            let result = edit(&mut settings.terminal_triggers);
            cx.notify();
            result
        })
    }

    pub(in crate::workspace) fn close_terminal_trigger_scope_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.edit_terminal_trigger(true, |state| state.scope_picker_open = false, cx);
    }

    fn persist_terminal_trigger_shell_execution(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let previous_settings = self.settings_store.settings().clone();
        let mut next_settings = previous_settings.clone();
        set_terminal_trigger_shell_execution(&mut next_settings, enabled);
        match self.settings_store.replace_and_save(next_settings) {
            Ok(saved) => {
                self.edit_terminal_trigger(
                    false,
                    |state| {
                        state.shell_confirmation_pending = false;
                        state.error_key = None;
                    },
                    cx,
                );
                self.apply_loaded_settings_to_runtime(&previous_settings, &saved.settings, cx);
                self.settings_workspace.update(cx, |settings, _cx| {
                    settings.acknowledge_external_store_state()
                });
                self.emit_native_plugin_settings_events(&previous_settings, &saved.settings, cx);
                self.refresh_terminal_trigger_runtime(cx);
                self.sync_tab_titles(cx);
            }
            Err(_error) => {
                self.edit_terminal_trigger(
                    false,
                    |state| {
                        state.shell_confirmation_pending = false;
                        state.error_key = Some("settings_view.terminal.triggers.save_failed");
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_trigger_editor(&mut self, cx: &mut Context<Self>) {
        self.edit_terminal_trigger(true, TerminalTriggersSettingsState::cancel_edit, cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn handle_terminal_trigger_request(
        &mut self,
        request: TriggerRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let TriggerRequest::ToggleForPane(pane, id) = request {
            self.toggle_terminal_trigger_for_pane(pane, &id, cx);
            cx.notify();
            return;
        }
        let opening = request.opens_editor();
        let structural = opening
            || matches!(
                request,
                TriggerRequest::CloseEditor | TriggerRequest::SaveEditor
            );
        if opening {
            self.prepare_modal_interaction_boundary(cx);
        }
        let shell_enabled = self
            .settings_store
            .settings()
            .terminal
            .triggers
            .explicit_shell_enabled;
        let outcome = self.settings_workspace.update(cx, |settings, cx| {
            settings.apply_terminal_trigger_request(request, shell_enabled, cx)
        });
        if outcome.clear_ime {
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
        if opening {
            window.focus(&self.focus_handle, cx);
        }
        if outcome.refresh_runtime {
            self.refresh_terminal_trigger_runtime(cx);
        }
        if let Some(enabled) = outcome.shell_trust {
            self.persist_terminal_trigger_shell_execution(enabled, cx);
        }
        if structural {
            cx.notify();
        }
    }
}
