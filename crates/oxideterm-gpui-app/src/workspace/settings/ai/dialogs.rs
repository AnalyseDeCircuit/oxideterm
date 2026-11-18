use super::*;
use oxideterm_gpui_settings_view::ai::confirm::AiSettingsConfirmIntent;

impl WorkspaceApp {
    pub(in crate::workspace) fn apply_ai_settings_confirmation(
        &mut self,
        intent: &AiSettingsConfirmIntent,
        cx: &mut Context<Self>,
    ) {
        match intent {
            AiSettingsConfirmIntent::Enable => self.edit_settings(
                |settings| {
                    settings.ai.enabled = true;
                    settings.ai.enabled_confirmed = true;
                },
                cx,
            ),
            AiSettingsConfirmIntent::RemoveProviderKey { provider_id } => {
                self.remove_ai_provider_api_key(provider_id, cx);
            }
            AiSettingsConfirmIntent::RemoveProvider { provider_id } => {
                self.remove_ai_provider(provider_id, cx)
            }
        }
    }

    pub(in crate::workspace) fn open_ai_settings_confirm(
        &mut self,
        intent: AiSettingsConfirmIntent,
        provider_name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.reset_standard_confirm_focus();
        self.settings_views.ai_confirm.update(cx, |dialog, cx| {
            dialog.open(intent, provider_name, self.tokens, self.i18n.clone(), cx);
        });
    }

    pub(in crate::workspace) fn handle_ai_settings_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_views.ai_confirm.read(cx).state().is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.settings_views
                        .ai_confirm
                        .update(cx, |dialog, cx| dialog.begin_exit(false, cx));
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.settings_views
                        .ai_confirm
                        .update(cx, |dialog, cx| dialog.begin_exit(true, cx));
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => {
                    self.settings_views.ai_confirm.update(cx, |dialog, cx| {
                        dialog.set_focus(self.standard_confirm_focus(), cx)
                    });
                    true
                }
                None => false,
            }
        } else {
            false
        }
    }

    pub(in crate::workspace) fn remove_ai_provider(
        &mut self,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .settings_store
            .settings()
            .ai
            .providers
            .iter()
            .position(|provider| ai_provider_id(provider).as_deref() == Some(provider_id))
        else {
            cx.notify();
            return;
        };
        let provider_id = provider_id.to_string();
        self.settings_workspace.update(cx, |state, cx| {
            state.ai_context_windows.remove_provider(&provider_id);
            cx.notify();
        });
        self.ai.state.update(cx, |ai, _| {
            ai.invalidate_provider_key_status(&provider_id);
            ai.invalidate_selector_provider_status(&provider_id);
        });
        self.settings_views
            .ai_providers
            .update(cx, |view, cx| view.remove_provider(&provider_id, cx));
        self.edit_settings(
            |settings| {
                ai_remove_provider_at_with_scoped_settings(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    &mut settings.ai.reasoning_provider_overrides,
                    &mut settings.ai.reasoning_model_overrides,
                    &mut settings.ai.user_context_windows,
                    index,
                );
            },
            cx,
        );

        self.ai.state.update(cx, |ai, cx| {
            ai.remove_provider_key(provider_id, cx);
        });
    }
}
