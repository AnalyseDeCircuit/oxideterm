use super::*;
pub(in crate::workspace) use oxideterm_gpui_settings_view::terminal_triggers::TerminalTriggersSettingsState;
pub(super) use oxideterm_gpui_settings_view::terminal_triggers::is_terminal_trigger_input;
use oxideterm_gpui_settings_view::terminal_triggers::{
    TerminalTriggerEditor, TerminalTriggerPage as TriggerPage,
    TerminalTriggerRequest as TriggerRequest,
};
use oxideterm_terminal_triggers::TerminalTrigger;

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_terminal_trigger_editor(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalTriggerEditor> {
        let owner = cx.entity().downgrade();
        let render_body = Rc::new(move |cx: &App| {
            let owner = owner.upgrade()?;
            let app = owner.read(cx);
            Some(app.with_terminal_trigger_page(owner, cx, |page, cx| {
                page.render_terminal_trigger_editor_modal(cx)
            }))
        });
        cx.new(|cx| TerminalTriggerEditor::new(settings, render_body, cx))
    }

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
        if let Some(item_ix) = settings_section_list_item_index(
            SettingsTab::Terminal,
            SettingsSectionId::TerminalAwarenessTriggers,
            self.settings_dynamic_section_counts(cx),
        ) {
            self.settings_workspace
                .read(cx)
                .section_list
                .state()
                .scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
        }
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn clear_terminal_trigger_input_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let input = self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input();
        if input.is_some_and(is_terminal_trigger_input) {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.blur_settings_entity_input(cx);
            });
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

    fn persist_terminal_trigger_change(&mut self, changed: bool, cx: &mut Context<Self>) {
        if changed {
            self.refresh_terminal_trigger_runtime(cx);
        }
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

    pub(in crate::workspace) fn terminal_triggers_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(pane_id) = self.settings_workspace.read(cx).terminal_triggers.pane {
            return self.terminal_trigger_session_card(pane_id, cx);
        }
        self.terminal_trigger_global_card(cx)
    }

    fn open_new_terminal_trigger_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare_modal_interaction_boundary(cx);
        self.edit_terminal_trigger(true, TerminalTriggersSettingsState::new_rule, cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn open_edit_terminal_trigger_editor(
        &mut self,
        trigger_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        self.edit_terminal_trigger(true, |state| state.edit_rule(trigger_id), cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn open_duplicate_terminal_trigger_editor(
        &mut self,
        trigger_id: &str,
        copy_suffix: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        self.edit_terminal_trigger(
            true,
            |state| state.duplicate_rule(trigger_id, copy_suffix),
            cx,
        );
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_trigger_editor(&mut self, cx: &mut Context<Self>) {
        self.edit_terminal_trigger(true, TerminalTriggersSettingsState::cancel_edit, cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    fn save_terminal_trigger_editor(&mut self, cx: &mut Context<Self>) {
        let input_valid = self
            .settings_workspace
            .update(cx, |settings, _| settings.apply_active_trigger_input());
        if !input_valid {
            self.edit_terminal_trigger(
                false,
                |state| {
                    state.error_key = Some("settings_view.terminal.triggers.validation_failed");
                },
                cx,
            );
            return;
        }
        let shell_enabled = self
            .settings_store
            .settings()
            .terminal
            .triggers
            .explicit_shell_enabled;
        let changed =
            self.edit_terminal_trigger(false, |state| state.save_editor(shell_enabled), cx);
        if changed {
            self.clear_terminal_trigger_input_focus(cx);
            self.ime_marked_text = None;
        }
        self.persist_terminal_trigger_change(changed, cx);
    }

    fn with_terminal_trigger_page(
        &self,
        workspace: Entity<Self>,
        cx: &App,
        render: impl FnOnce(TriggerPage<'_>, &App) -> AnyElement,
    ) -> AnyElement {
        let owner = workspace.downgrade();
        let pane_exists = |pane, cx: &App| self.tab_host.read(cx).panes().contains_key(&pane);
        let applies_to_pane = |pane, trigger: &TerminalTrigger, cx: &App| {
            self.terminal_trigger_applies_to_pane(pane, trigger, cx)
        };
        let enabled_for_pane = |pane, trigger: &str, cx: &App| {
            self.terminal_trigger_effective_enabled_for_pane(pane, trigger, cx)
        };
        let quick_command_label = |selected: &str, cx: &App| {
            self.terminal
                .read(cx)
                .quick_commands
                .store
                .commands
                .iter()
                .find(|command| command.id == selected)
                .map(|command| command.name.clone())
        };
        self.with_settings_controls_for(workspace, cx, |controls, cx| {
            render(
                TriggerPage {
                    controls,
                    terminal_triggers: &self.settings_workspace.read(cx).terminal_triggers,
                    connection_store: &self.connection_store,
                    terminal_trigger_shell_confirmation_pending: self
                        .settings_workspace
                        .read(cx)
                        .terminal_triggers
                        .shell_confirmation_pending,
                    modal_scroll: self
                        .settings_workspace
                        .read(cx)
                        .terminal_triggers
                        .modal_scroll
                        .clone(),
                    pane_exists: &pane_exists,
                    applies_to_pane: &applies_to_pane,
                    enabled_for_pane: &enabled_for_pane,
                    quick_command_label: &quick_command_label,
                    dispatch_trigger: Rc::new(move |request, window, cx| {
                        let _ = owner.update(cx, |app, cx| {
                            app.handle_terminal_trigger_request(request, window, cx)
                        });
                    }),
                },
                cx,
            )
        })
    }

    fn handle_terminal_trigger_request(
        &mut self,
        request: TriggerRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            TriggerRequest::OpenNew => self.open_new_terminal_trigger_editor(window, cx),
            TriggerRequest::OpenEdit(id) => self.open_edit_terminal_trigger_editor(&id, window, cx),
            TriggerRequest::OpenDuplicate(id, suffix) => {
                self.open_duplicate_terminal_trigger_editor(&id, &suffix, window, cx)
            }
            TriggerRequest::CloseEditor => self.close_terminal_trigger_editor(cx),
            TriggerRequest::SaveEditor => self.save_terminal_trigger_editor(cx),
            TriggerRequest::CloseScopePicker => self.close_terminal_trigger_scope_picker(cx),
            TriggerRequest::ShowGlobal => {
                self.edit_terminal_trigger(false, |state| state.pane = None, cx)
            }
            TriggerRequest::ToggleShellTrust(enabled) => {
                if enabled {
                    self.persist_terminal_trigger_shell_execution(false, cx);
                } else {
                    self.edit_terminal_trigger(
                        false,
                        |state| state.shell_confirmation_pending = true,
                        cx,
                    );
                }
            }
            TriggerRequest::CancelShellTrust => {
                self.edit_terminal_trigger(
                    false,
                    |state| state.shell_confirmation_pending = false,
                    cx,
                );
            }
            TriggerRequest::ConfirmShellTrust => {
                self.persist_terminal_trigger_shell_execution(true, cx)
            }
            TriggerRequest::ToggleRule(id) => {
                let changed = self.edit_terminal_trigger(false, |state| state.toggle_rule(&id), cx);
                self.persist_terminal_trigger_change(changed, cx);
            }
            TriggerRequest::DeleteRule(id) => {
                let changed = self.edit_terminal_trigger(false, |state| state.delete_rule(&id), cx);
                self.persist_terminal_trigger_change(changed, cx);
            }
            TriggerRequest::ToggleForPane(pane, id) => {
                self.toggle_terminal_trigger_for_pane(pane, &id, cx);
                cx.notify();
            }
            TriggerRequest::EditDraft { clear_input, edit } => {
                self.edit_terminal_trigger(clear_input, edit, cx)
            }
        }
    }

    fn terminal_trigger_global_card(&self, cx: &mut Context<Self>) -> AnyElement {
        self.with_terminal_trigger_page(cx.entity(), cx, |page, cx| {
            page.terminal_trigger_global_card(cx)
        })
    }

    fn terminal_trigger_session_card(&self, pane: PaneId, cx: &mut Context<Self>) -> AnyElement {
        self.with_terminal_trigger_page(cx.entity(), cx, |page, cx| {
            page.terminal_trigger_session_card(pane, cx)
        })
    }
}
