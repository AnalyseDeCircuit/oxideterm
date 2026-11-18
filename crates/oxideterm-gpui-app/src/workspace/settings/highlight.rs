use super::*;
use oxideterm_gpui_settings_view::highlight_page::{
    HighlightEdit, HighlightFileOperation, HighlightFileResult, HighlightModal, HighlightOutcome,
    HighlightRequest,
};

pub(in crate::workspace) fn is_semantic_scheme_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::SemanticSchemeName
            | SettingsInput::SemanticSchemeRulePattern(_)
            | SettingsInput::SemanticSchemeRuleCapture(_)
            | SettingsInput::SemanticSchemeColor(_)
    )
}

pub(in crate::workspace) fn is_highlight_rule_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::HighlightLabel(_)
            | SettingsInput::HighlightPattern(_)
            | SettingsInput::HighlightForeground(_)
            | SettingsInput::HighlightBackground(_)
    )
}

impl WorkspaceApp {
    pub(super) fn blur_highlight_input(
        &mut self,
        owns: fn(SettingsInput) -> bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            .is_some_and(owns)
        {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.blur_settings_entity_input(cx);
            });
            self.clear_ime_selection();
        }
    }

    pub(in crate::workspace) fn open_semantic_scheme_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings_store
            .settings()
            .terminal
            .active_custom_semantic_scheme()
            .is_none()
        {
            return;
        }
        self.prepare_modal_interaction_boundary(cx);
        self.blur_highlight_input(is_semantic_scheme_input, cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.semantic_scheme_editor_open = true;
            cx.notify();
        });
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_semantic_scheme_editor(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.semantic_scheme_editor_open = false;
            cx.notify();
        });
        self.blur_highlight_input(is_semantic_scheme_input, cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn open_highlight_rule_editor(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index
            >= self
                .settings_store
                .settings()
                .terminal
                .effective_highlight_rules()
                .len()
        {
            return;
        }
        self.prepare_modal_interaction_boundary(cx);
        self.blur_highlight_input(is_highlight_rule_input, cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.highlight_rule_editor_index = Some(index);
            cx.notify();
        });
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_highlight_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight.highlight_rule_editor_index = None;
            cx.notify();
        });
        self.blur_highlight_input(is_highlight_rule_input, cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn handle_highlight_request(
        &mut self,
        request: HighlightRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.settings_workspace.update(cx, |settings, cx| {
            settings.highlight_request(
                request,
                self.settings_store.settings(),
                &self.connection_store,
                &self.i18n,
                cx,
            )
        });
        match outcome {
            HighlightOutcome::None => {}
            HighlightOutcome::Transfer(operation) => self.transfer_highlight_file(operation, cx),
            HighlightOutcome::Modal(modal) => self.apply_highlight_modal(modal, window, cx),
            HighlightOutcome::Warning(message) => {
                self.send_settings_notice(message, TerminalNoticeVariant::Error, cx);
            }
            HighlightOutcome::Edit(edit) => {
                if let HighlightEdit::Remove(index) = &edit
                    && self
                        .settings_workspace
                        .read(cx)
                        .highlight
                        .highlight_rule_editor_index
                        == Some(*index)
                {
                    self.close_highlight_rule_editor(cx);
                }
                let mut result = Ok(None);
                self.edit_settings(|settings| result = edit.apply(settings), cx);
                match result {
                    Ok(Some(modal)) => self.apply_highlight_modal(modal, window, cx),
                    Ok(None) => {}
                    Err(error) => {
                        self.send_settings_notice(error, TerminalNoticeVariant::Error, cx)
                    }
                }
            }
        }
    }

    fn apply_highlight_modal(
        &mut self,
        modal: HighlightModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match modal {
            HighlightModal::OpenRule(index) => self.open_highlight_rule_editor(index, window, cx),
            HighlightModal::OpenSemantic => self.open_semantic_scheme_editor(window, cx),
            HighlightModal::CloseRule => self.close_highlight_rule_editor(cx),
            HighlightModal::CloseSemantic => self.close_semantic_scheme_editor(cx),
        }
    }

    fn transfer_highlight_file(
        &mut self,
        operation: HighlightFileOperation,
        cx: &mut Context<Self>,
    ) {
        let selection = oxideterm_gpui_settings_view::entity::picker::pick_path(
            cx,
            operation.prompt(&self.i18n),
            operation.picks_directory(),
        );
        let owner = cx.entity().downgrade();
        self.settings_workspace.update(cx, |_, cx| {
            operation.start(
                selection,
                self.forwarding_runtime.handle().clone(),
                &self.i18n,
                move |result, cx| {
                    let _ = owner.update(cx, |app, cx| {
                        let (file, success) = match result {
                            Ok(result) => result,
                            Err(error) => {
                                app.send_settings_notice(error, TerminalNoticeVariant::Error, cx);
                                return;
                            }
                        };
                        if matches!(file, HighlightFileResult::Exported) {
                            app.send_settings_notice(success, TerminalNoticeVariant::Success, cx);
                            return;
                        }
                        let close_rule_editor = matches!(file, HighlightFileResult::RuleSet(_));
                        let mut applied = Ok(false);
                        app.edit_settings(|settings| applied = file.apply(settings), cx);
                        match applied {
                            Ok(true) => {
                                if close_rule_editor {
                                    app.close_highlight_rule_editor(cx);
                                }
                                app.send_settings_notice(
                                    success,
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                            }
                            Ok(false) => {}
                            Err(error) => {
                                app.send_settings_notice(error, TerminalNoticeVariant::Error, cx)
                            }
                        }
                    });
                },
                cx,
            );
        });
    }
}
