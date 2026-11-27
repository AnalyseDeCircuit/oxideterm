use super::super::*;
use oxideterm_gpui_settings_view::{
    components::{is_connection_key_input, is_surface_input},
    terminal_triggers::is_terminal_trigger_input,
};
use oxideterm_settings_model::{
    apply_cloud_sync_form_input_owned, cloud_sync_form_input_value_ref,
    persisted_settings_input_value, plugin_setting_input_value, take_cloud_sync_form_input_value,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_settings_input_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use oxideterm_gpui_settings_view::input::SettingsInputKeyEffect::*;
        if event.keystroke.key == "tab"
            && self
                .settings_workspace
                .read(cx)
                .settings_entity_focused_input()
                .is_some_and(SettingsInput::is_ai_mcp)
        {
            if let Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(action)) =
                browser_behavior::modal_footer_input_key_action(
                    "tab",
                    event.keystroke.modifiers.shift,
                    &CONFIRM_DIALOG_FOOTER_ACTIONS,
                    true,
                    true,
                    self.standard_confirm_focus_owner(),
                    ConfirmDialogAction::Cancel,
                    None,
                )
            {
                self.settings_workspace
                    .update(cx, |state, cx| state.blur_settings_entity_input(cx));
                self.set_standard_confirm_focus(action);
                self.settings_views
                    .ai_mcp_dialog
                    .update(cx, |view, cx| view.set_focus(Some(action), cx));
                self.show_active_input_caret(cx);
            }
            return true;
        }
        if self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            .is_some()
        {
            let effect = self
                .settings_workspace
                .update(cx, |settings, cx| settings.handle_settings_key(event, cx));
            match effect {
                Ignored | Handled => {}
                CloseSearch => self.close_settings_search(cx),
                SubmitSearch => {
                    self.activate_first_settings_search_result(cx);
                }
                CloseTriggerScope => self.close_terminal_trigger_scope_picker(cx),
                CloseTriggerEditor => self.close_terminal_trigger_editor(cx),
                CloseSemantic => self.close_semantic_scheme_editor(cx),
                CloseHighlight => self.close_highlight_rule_editor(cx),
                CloseCommandSpecs => {
                    self.close_terminal_command_specs_editor(cx);
                    self.show_active_input_caret(cx);
                }
                ClearIme => {
                    self.clear_ime_selection();
                    self.show_active_input_caret(cx);
                }
            }
            return true;
        }
        if let Some(input) = self.knowledge_page(cx).read(cx).focused_settings_input() {
            let key = event.keystroke.key.as_str();
            let modifiers = event.keystroke.modifiers;
            if matches!(key, "tab" | "enter" | "escape") {
                match input {
                    SettingsInput::KnowledgeDocumentTitle => {
                        if modifiers.platform || modifiers.control {
                            // A modified Enter is not the dialog's submit gesture;
                            // keep focus in the title instead of falling through to
                            // the generic blur path below.
                            return true;
                        }
                        if self.handle_knowledge_document_dialog_key(event, cx) {
                            return true;
                        }
                    }
                    SettingsInput::KnowledgeCollectionName => {
                        if modifiers.platform || modifiers.control {
                            return true;
                        }
                        if self.handle_knowledge_collection_dialog_key(event, cx) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            match key {
                "escape" | "enter" => {
                    self.knowledge_page(cx).update(cx, |ai, cx| {
                        ai.blur_settings_input(cx);
                    });
                    self.clear_ime_selection();
                    self.show_active_input_caret(cx);
                    return true;
                }
                "backspace" | "delete" if !modifiers.platform && !modifiers.control => {
                    self.knowledge_page(cx).update(cx, |ai, cx| {
                        ai.pop_settings_input(input, cx);
                    });
                    return true;
                }
                _ => return true,
            }
        }
        let Some(input) = self.focused_settings_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        match key {
            "escape" => {
                if matches!(
                    input,
                    SettingsInput::AppLockCurrentPassword
                        | SettingsInput::AppLockNewPassword
                        | SettingsInput::AppLockConfirmPassword
                ) {
                    self.commit_focused_app_lock_input();
                } else if self.commit_focused_cloud_sync_input(input, cx) {
                    self.focused_settings_input = None;
                } else {
                    self.focused_settings_input = None;
                    self.clear_settings_input_draft(input);
                }
                self.show_active_input_caret(cx);
                cx.notify();
                true
            }
            "enter" => {
                if matches!(
                    input,
                    SettingsInput::NativePluginInstalledPageJump
                        | SettingsInput::NativePluginMarketplacePageJump
                ) {
                    self.commit_plugin_page_jump(cx);
                    return true;
                }
                if matches!(
                    input,
                    SettingsInput::NativePluginInstalledPageSize
                        | SettingsInput::NativePluginMarketplacePageSize
                ) {
                    self.commit_plugin_page_size(cx);
                    return true;
                }
                if input.accepts_newline() {
                    self.settings_input_draft.push('\n');
                    self.apply_settings_input_draft(input, cx);
                    return true;
                }
                if matches!(
                    input,
                    SettingsInput::AppLockCurrentPassword
                        | SettingsInput::AppLockNewPassword
                        | SettingsInput::AppLockConfirmPassword
                ) {
                    self.commit_focused_app_lock_input();
                } else if self.commit_focused_cloud_sync_input(input, cx) {
                    self.focused_settings_input = None;
                } else {
                    self.focused_settings_input = None;
                    self.clear_settings_input_draft(input);
                }
                self.show_active_input_caret(cx);
                cx.notify();
                true
            }
            "backspace" | "delete" if !modifiers.platform && !modifiers.control => {
                if self.settings_input_draft.pop().is_some() {
                    // Empty Backspace/Delete does not change the draft value.
                    self.apply_settings_input_draft(input, cx);
                }
                true
            }
            _ => true,
        }
    }

    pub(in crate::workspace) fn blur_host_tools_input(&mut self, cx: &mut App) -> bool {
        let changed = self
            .host_tools
            .update(cx, |host_tools, cx| host_tools.blur_input(cx));
        if changed {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        changed
    }

    pub(in crate::workspace) fn blur_text_inputs(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.search.blur();
        if changed {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        changed |= self.blur_host_tools_input(cx);
        if self.terminal_command_sender.read(cx).compact_focused() {
            self.terminal_command_sender.update(cx, |sender, cx| {
                sender.set_compact_focused(false, cx);
            });
            self.ime_marked_text = None;
            changed = true;
        }
        let blurred_settings = self.settings_workspace.update(cx, |settings, cx| {
            let focused = settings.settings_entity_focused_input();
            settings.blur_settings_entity_input(cx);
            focused
        });
        if let Some(input) = blurred_settings {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            if is_surface_input(input)
                || is_connection_key_input(input)
                || oxideterm_gpui_settings_view::theme_editor::is_theme_editor_input(input)
            {
                self.show_active_input_caret(cx);
            } else {
                changed = true;
            }
        }
        if self
            .knowledge_page(cx)
            .update(cx, |ai, cx| ai.blur_settings_input(cx))
        {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            changed = true;
        }
        if let Some(input) = self.focused_settings_input.take() {
            if matches!(
                input,
                SettingsInput::AppLockCurrentPassword
                    | SettingsInput::AppLockNewPassword
                    | SettingsInput::AppLockConfirmPassword
            ) {
                self.focused_settings_input = Some(input);
                self.commit_focused_app_lock_input();
            } else if self.commit_focused_cloud_sync_input(input, cx) {
                // Cloud Sync fields move out of their Entity while focused, so
                // every blur boundary must return the owned draft before release.
            } else {
                self.clear_settings_input_draft(input);
            }
            self.ime_marked_text = None;
            self.clear_ime_selection();
            changed = true;
        }
        if self.open_settings_select.is_some() {
            self.ime_marked_text = None;
            self.close_settings_select();
            changed = true;
        }
        if self.connection_form_state(cx).open_select.is_some() {
            self.ime_marked_text = None;
            self.close_new_connection_select(cx);
            changed = true;
        }
        if self
            .terminal
            .update(cx, |terminal, _cx| terminal.blur_cast_search())
        {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.blur_terminal_quick_commands_input(cx) {
            changed = true;
        }
        if self.close_terminal_git_branch_picker(cx) {
            changed = true;
        }
        if self
            .connections
            .session_manager
            .update(cx, |session_manager, cx| {
                session_manager.clear_input_focus(cx)
            })
        {
            self.ime_marked_text = None;
            changed = true;
        }
        if let Some(page) = self.forwarding_page(cx)
            && page.update(cx, |page, cx| {
                let changed = page.clear_input_focus();
                if changed {
                    cx.notify();
                }
                changed
            })
        {
            self.ime_marked_text = None;
            changed = true;
        }
        if self
            .file_manager
            .update(cx, |file_manager, cx| file_manager.clear_input_focus(cx))
        {
            self.ime_marked_text = None;
            changed = true;
        }
        if self
            .graphics
            .update(cx, |graphics, cx| graphics.clear_input_focus(cx))
        {
            self.ime_marked_text = None;
            changed = true;
        }
        if self
            .sftp_view()
            .update(cx, |sftp, cx| sftp.clear_input_focus(cx))
        {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.state.read(cx).model_selector_search_focused()
            || self.ai.state.read(cx).model_selector_open()
        {
            // The AI model selector can live either in the sidebar portal or
            // inside the terminal inline panel. A generic outside blur should
            // release the searchable select without restoring inline focus.
            self.ai.state.update(cx, |ai, _cx| {
                ai.close_model_selector();
            });
            self.ime_marked_text = None;
            changed = true;
        }
        if self
            .ai
            .state
            .read(cx)
            .terminal_inline_panel()
            .prompt_focused
        {
            // The inline AI prompt is rendered inside the terminal pane rather
            // than as a normal form control, so it must explicitly join the
            // shared blur path or it remains the active IME target after an
            // outside click.
            self.ai.state.update(cx, |ai, _cx| {
                ai.terminal_inline_panel_mut().prompt_focused = false;
            });
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.state.read(cx).chat_ui().input_focused {
            self.ai.state.update(cx, |ai, _cx| {
                ai.blur_chat_input(true);
            });
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.state.read(cx).chat_ui().editing_message_focused {
            self.ai.state.update(cx, |ai, _cx| {
                ai.blur_message_edit();
            });
            self.ime_marked_text = None;
            changed = true;
        }
        let blurred_connection_form = self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return false;
            };
            if !form.field_focused {
                return false;
            }
            form.field_focused = false;
            form.selected_field = None;
            true
        });
        if blurred_connection_form {
            self.ime_marked_text = None;
            changed = true;
        }
        if changed {
            self.clear_ime_selection();
            self.show_active_input_caret(cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn focus_settings_input(
        &mut self,
        input: SettingsInput,
        current_value: String,
        cx: &mut Context<Self>,
    ) {
        let dismissed_select = self.open_settings_select.is_some();
        self.close_settings_select();
        let trigger_input = is_terminal_trigger_input(input);
        let highlight_input =
            oxideterm_gpui_settings_view::highlight_page::is_highlight_input(input);
        let entity_owned_input = trigger_input
            || highlight_input
            || self
                .settings_workspace
                .read(cx)
                .settings_entity_input_value(input)
                .is_some();
        if entity_owned_input {
            self.knowledge_page(cx).update(cx, |ai, cx| {
                ai.blur_settings_input(cx);
            });
            if let Some(previous_input) = self.focused_settings_input.take() {
                self.clear_settings_input_draft(previous_input);
            }
            self.settings_workspace.update(cx, |settings, cx| {
                if highlight_input {
                    settings.focus_highlight_input(input, current_value, cx);
                } else {
                    settings.focus_settings_entity_input(input, cx);
                }
            });
            self.clear_ime_selection();
            self.show_active_input_caret(cx);
            if (!trigger_input
                && !highlight_input
                && !oxideterm_gpui_settings_view::theme_editor::is_theme_editor_input(input)
                && !is_surface_input(input)
                && !oxideterm_gpui_settings_view::ai::providers::is_provider_draft(input)
                && !input.is_ai_mcp()
                && !is_connection_key_input(input)
                && !matches!(
                    input,
                    SettingsInput::TerminalCommandSpecsJson
                        | SettingsInput::PortableCurrentPassword
                        | SettingsInput::PortableNewPassword
                        | SettingsInput::PortableConfirmPassword
                ))
                || dismissed_select
            {
                cx.notify();
            }
            return;
        }
        if oxideterm_gpui_ai::knowledge::page::KnowledgePage::owns_settings_input(input) {
            if let Some(previous_input) = self.focused_settings_input.take() {
                self.clear_settings_input_draft(previous_input);
            }
            self.settings_workspace.update(cx, |settings, cx| {
                settings.blur_settings_entity_input(cx);
            });
            self.knowledge_page(cx).update(cx, |ai, cx| {
                ai.focus_settings_input(input, cx);
            });
            self.clear_ime_selection();
            self.show_active_input_caret(cx);
            cx.notify();
            return;
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.blur_settings_entity_input(cx);
        });
        self.knowledge_page(cx).update(cx, |ai, cx| {
            ai.blur_settings_input(cx);
        });
        let app_lock_input = matches!(
            input,
            SettingsInput::AppLockCurrentPassword
                | SettingsInput::AppLockNewPassword
                | SettingsInput::AppLockConfirmPassword
        );
        let cloud_sync_input = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync_form_input_value_ref(&cloud_sync.view.form, input).is_some()
        };
        if (app_lock_input || cloud_sync_input) && self.focused_settings_input == Some(input) {
            // Repositioning the caret in a manually owned input must preserve
            // the active draft instead of taking the now-empty backing field.
            self.clear_ime_selection();
            self.show_active_input_caret(cx);
            cx.notify();
            return;
        }
        if let Some(previous_input) = self
            .focused_settings_input
            .filter(|previous| *previous != input)
        {
            if matches!(
                previous_input,
                SettingsInput::AppLockCurrentPassword
                    | SettingsInput::AppLockNewPassword
                    | SettingsInput::AppLockConfirmPassword
            ) {
                self.commit_focused_app_lock_input();
            } else if self.commit_focused_cloud_sync_input(previous_input, cx) {
                self.focused_settings_input = None;
            } else {
                self.clear_settings_input_draft(previous_input);
            }
        }
        self.focused_settings_input = Some(input);
        self.clear_ime_selection();
        self.settings_input_draft = if app_lock_input {
            // Move the active secret into the editor so only one owner exists.
            self.take_app_lock_input_value(input).unwrap_or_default()
        } else if cloud_sync_input {
            // Cloud Sync form values move into the root only while it acts as
            // the focused IME adapter. No second secret buffer is created.
            self.cloud_sync
                .update(cx, |cloud_sync, _cx| {
                    take_cloud_sync_form_input_value(&mut cloud_sync.view.form, input)
                })
                .unwrap_or_default()
        } else {
            current_value
        };
        self.show_active_input_caret(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn clear_settings_input_draft(&mut self, input: SettingsInput) {
        if input.is_secret() || input == SettingsInput::TerminalCommandSpecsJson {
            zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
        }
        self.settings_input_draft.clear();
    }

    pub(in crate::workspace) fn apply_focused_cloud_sync_input_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.focused_settings_input else {
            return false;
        };
        if !self.commit_focused_cloud_sync_input(input, cx) {
            return false;
        }
        // Cloud Sync configuration commits can be triggered by tab/action
        // changes, so release the manually owned input after its latest draft
        // has been copied into the form.
        self.focused_settings_input = None;
        self.clear_settings_input_draft(input);
        true
    }

    fn commit_focused_cloud_sync_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        let draft = std::mem::take(&mut self.settings_input_draft);
        match self.cloud_sync.update(cx, |cloud_sync, cx| {
            let result = apply_cloud_sync_form_input_owned(&mut cloud_sync.view.form, input, draft);
            if result.is_ok() {
                cx.notify();
            }
            result
        }) {
            Ok(()) => true,
            Err(draft) => {
                self.settings_input_draft = draft;
                false
            }
        }
    }

    pub(in crate::workspace) fn current_settings_input_value(
        &self,
        input: SettingsInput,
        cx: &Context<Self>,
    ) -> String {
        if input.is_secret() {
            // Generic render/focus snapshots must never duplicate credentials.
            // Secret owners expose masked views or move the draft at focus.
            return String::new();
        }
        let settings = self.settings_store.settings();
        if let SettingsInput::LocalShellPath(index) = input {
            return self
                .effective_local_shells_for_settings(settings)
                .get(index)
                .and_then(|shell| {
                    settings
                        .local_terminal
                        .shell_paths
                        .get(&shell.id)
                        .or_else(|| {
                            if shell.id == "git-bash" {
                                settings.local_terminal.git_bash_path.as_ref()
                            } else {
                                None
                            }
                        })
                })
                .cloned()
                .unwrap_or_default();
        }
        if let Some(value) = persisted_settings_input_value(settings, input) {
            return value;
        }
        if is_terminal_trigger_input(input) {
            return String::new();
        }
        if let Some(value) = self.knowledge_page(cx).read(cx).settings_input_value(input) {
            return value.to_owned();
        }
        if let Some(value) = self
            .settings_workspace
            .read(cx)
            .settings_entity_input_value(input)
        {
            // This copy is only made at an explicit focus/action boundary;
            // render paths borrow the Entity-owned theme draft directly.
            return value.to_owned();
        }
        if let Some(value) =
            cloud_sync_form_input_value_ref(&self.cloud_sync.read(cx).view.form, input)
        {
            // Secret fields are moved into the focused IME adapter through
            // `focus_settings_input`; this generic snapshot path must not copy
            // their contents.
            return if input.is_secret() {
                String::new()
            } else {
                value.to_owned()
            };
        }
        match input {
            SettingsInput::PublicMcpPort => self.public_mcp.port_draft().to_owned(),
            SettingsInput::NativePluginInstallUrl => {
                self.plugin_manager_state(cx).install_url_draft.clone()
            }
            SettingsInput::NativePluginInstallChecksum => {
                self.plugin_manager_state(cx).install_checksum_draft.clone()
            }
            SettingsInput::NativePluginRegistryUrl => {
                self.plugin_manager_state(cx).registry_url_draft.clone()
            }
            SettingsInput::NativePluginMarketplaceSearch => self
                .plugin_manager_state(cx)
                .marketplace_search_draft
                .clone(),
            SettingsInput::NativePluginInstalledPageSize => self
                .plugin_page_size(plugin_manager::NativePluginManagerTab::Installed, cx)
                .to_string(),
            SettingsInput::NativePluginMarketplacePageSize => self
                .plugin_page_size(plugin_manager::NativePluginManagerTab::Marketplace, cx)
                .to_string(),
            SettingsInput::NativePluginInstalledPageJump => self
                .plugin_page_number(plugin_manager::NativePluginManagerTab::Installed, cx)
                .to_string(),
            SettingsInput::NativePluginMarketplacePageJump => self
                .plugin_page_number(plugin_manager::NativePluginManagerTab::Marketplace, cx)
                .to_string(),
            SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword => String::new(),
            SettingsInput::AppLockCurrentPassword
            | SettingsInput::AppLockNewPassword
            | SettingsInput::AppLockConfirmPassword => String::new(),
            SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase
            | SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase
            | SettingsInput::ManagedKeyRenameName
            | SettingsInput::DiscoverySecret(_)
            | SettingsInput::DiscoveryField(_)
            | SettingsInput::DiscoveryMultiline(_)
            | SettingsInput::ConnectionImportTargetGroup => String::new(),
            SettingsInput::LocalPrivilegeLabel
            | SettingsInput::TotpName
            | SettingsInput::TotpSecret
            | SettingsInput::TotpPattern
            | SettingsInput::TotpPeriod
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint => String::new(),
            SettingsInput::PluginSetting(index) => self
                .plugin_entity
                .read(cx)
                .registry()
                .contributions()
                .settings
                .get(index)
                .and_then(|setting| {
                    self.plugin_entity
                        .read(cx)
                        .registry()
                        .plugin_setting_value(&setting.plugin_id, &setting.definition.id)
                })
                .map(|value| plugin_setting_input_value(&value))
                .unwrap_or_default(),
            _ => String::new(),
        }
    }
}
